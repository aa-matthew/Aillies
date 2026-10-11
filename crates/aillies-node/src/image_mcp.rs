//! Explicit image tool for Codex custom providers. Uses the shared retry budget.

use crate::{
    body::{self, Body},
    node::Node,
};
use aillies_core::{Failure, FailureCode};
use base64::{engine::general_purpose::STANDARD, Engine};
use bytes::Bytes;
use http_body_util::BodyExt;
use hyper::{body::Incoming, Request, Response};
use serde_json::{json, Value};
use std::sync::Arc;

const MAX_IMAGE_RESPONSE: usize = 64 * 1024 * 1024;

pub(crate) async fn handle(node: Arc<Node>, req: Request<Incoming>) -> Response<Body> {
    if req.method() != "POST" {
        return body::json(405, &json!({"error":"POST required"}));
    }
    // Browser-origin traffic is unnecessary for Codex and refused explicitly.
    if req.headers().contains_key("origin") {
        return body::failure(&Failure::new(FailureCode::Unauthorized));
    }
    let raw = match body::read_limited(req.into_body(), 32 * 1024).await {
        Ok(raw) => raw,
        Err(_) => return body::failure(&Failure::new(FailureCode::RequestInvalid)),
    };
    let msg: Value = match serde_json::from_slice(&raw) {
        Ok(Value::Object(msg)) => Value::Object(msg),
        _ => return rpc_error(Value::Null, -32700, "Invalid JSON-RPC request."),
    };
    let id = msg.get("id").cloned().unwrap_or(Value::Null);
    if msg["jsonrpc"] != "2.0" {
        return rpc_error(id, -32600, "Invalid JSON-RPC request.");
    }
    let method = msg["method"].as_str().unwrap_or("");
    if !msg.as_object().is_some_and(|m| m.contains_key("id")) {
        return if method == "notifications/initialized" { body::json(202, &Value::Null) } else { rpc_error(Value::Null, -32600, "Unsupported notification.") };
    }
    let result = match method {
        "initialize" => {
            let version =
                msg["params"]["protocolVersion"].as_str().filter(|v| matches!(*v, "2024-11-05" | "2025-03-26" | "2025-06-18")).unwrap_or("2025-03-26");
            json!({"protocolVersion":version,"capabilities":{"tools":{}},"serverInfo":{"name":"aillies-images","version":aillies_core::APP_VERSION},
                "instructions":"Use generate_image for image creation through the selected Aillies device. Show the returned Markdown image reference in your final reply. Reuse the saved path when asked to show the image again. Aillies retries failed requests up to three times on the same device and account; attempts may consume usage. Never automatically repeat a failed tool call; delivery may be uncertain."})
        }
        "ping" => json!({}),
        "tools/list" => {
            json!({"tools":[{"name":"generate_image","description":"Generate a PNG image using the selected Aillies device's Codex account. Use this when the user asks to create an image. Show the returned Markdown image reference in your final reply. Reuse the saved path when asked to show it again. Aillies handles up to three retries. Do not repeat failed tool calls automatically.",
            "inputSchema":{"type":"object","properties":{"prompt":{"type":"string","minLength":1,"maxLength":16000},"model":{"type":"string","minLength":1,"maxLength":100,"description":"Pass the current chat model. When omitted, uses the model saved in Codex settings."}},"required":["prompt"],"additionalProperties":false},
            "annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":true}}]})
        }
        "tools/call" => {
            if msg["params"]["name"] != "generate_image" {
                return rpc_error(id, -32602, "Unknown image tool.");
            }
            let args = &msg["params"]["arguments"];
            let prompt = args["prompt"].as_str().filter(|p| !p.trim().is_empty() && p.len() <= 16000);
            let model = args.get("model").and_then(Value::as_str);
            if args.as_object().is_none_or(|a| a.keys().any(|k| k != "prompt" && k != "model"))
                || prompt.is_none()
                || args.get("model").is_some_and(|_| model.is_none_or(|s| s.is_empty() || s.len() > 100))
            {
                return rpc_error(id, -32602, "Provide an image prompt and an optional model.");
            }
            match generate(node.clone(), prompt.unwrap_or(""), model).await {
                Ok(result) => result,
                Err(f) => {
                    node.record_failure(&f);
                    json!({"isError":true,"content":[{"type":"text","text":format!("{} Do not retry automatically.",f.message())}]})
                }
            }
        }
        _ => return rpc_error(id, -32601, "Method not found."),
    };
    body::json(200, &json!({"jsonrpc":"2.0","id":id,"result":result}))
}

fn rpc_error(id: Value, code: i32, message: &'static str) -> Response<Body> {
    body::json(200, &json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}}))
}

async fn generate(node: Arc<Node>, prompt: &str, model: Option<&str>) -> Result<Value, Failure> {
    let model = match model {
        Some(model) => model.to_owned(),
        None => node.codex_config().selected_model().map_err(|_| Failure::new(FailureCode::RequestInvalid))?,
    };
    let request = json!({"model":model,"input":prompt,"tools":[{"type":"image_generation","output_format":"png"}],"tool_choice":{"type":"image_generation"},"stream":true});
    let raw = serde_json::to_vec(&request).map_err(|_| Failure::new(FailureCode::RequestInvalid))?;
    let response = crate::loopback::route_turn(node.clone(), Bytes::from(raw)).await;
    let status = response.status().as_u16();
    let received = tokio::time::timeout(std::time::Duration::from_secs(890), http_body_util::Limited::new(response.into_body(), MAX_IMAGE_RESPONSE).collect())
        .await
        .map_err(|_| Failure::new(FailureCode::DeliveryUncertain))?
        .map_err(|_| Failure::new(FailureCode::DeliveryUncertain))?
        .to_bytes();
    if status != 200 {
        return Err(Failure::from_json(&received).unwrap_or(Failure::new(FailureCode::DeliveryUncertain)));
    }
    let encoded = extract_image(&received)?;
    let png = STANDARD.decode(&encoded).map_err(|_| Failure::new(FailureCode::StreamFailed))?;
    validate_png(&png)?;
    let path = node.codex_config().codex_home.join("generated_images/aillies").join(format!("{}.png", uuid::Uuid::new_v4()));
    let path = std::path::absolute(path).map_err(|_| Failure::new(FailureCode::StreamFailed))?;
    crate::store::private_write(&path, &png).map_err(|_| Failure::new(FailureCode::StreamFailed))?;
    let reference = image_reference(&path);
    Ok(
        json!({"content":[{"type":"image","mimeType":"image/png","data":encoded},{"type":"text","text":format!("Image saved to {}\nShow this image in your final reply:\n{reference}\nReuse this saved image if asked to show it again.",path.display())}]}),
    )
}

fn image_reference(path: &std::path::Path) -> String {
    let path = path.to_string_lossy();
    #[cfg(windows)]
    let path = path.replace('\\', "/");
    // Angle brackets allow spaces; encode characters that can break the target.
    let path = path.replace('%', "%25").replace('<', "%3C").replace('>', "%3E").replace('\r', "%0D").replace('\n', "%0A");
    format!("![Generated image](<{path}>)")
}

fn validate_png(bytes: &[u8]) -> Result<(), Failure> {
    let invalid = || Failure::new(FailureCode::StreamFailed);
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_limits(png::Limits { bytes: MAX_IMAGE_RESPONSE });
    let mut reader = decoder.read_info().map_err(|_| invalid())?;
    if reader.output_buffer_size() > MAX_IMAGE_RESPONSE {
        return Err(invalid());
    }
    let mut pixels = vec![0; reader.output_buffer_size()];
    reader.next_frame(&mut pixels).map_err(|_| invalid())?;
    reader.finish().map_err(|_| invalid())?;
    Ok(())
}

pub(crate) fn validate_response(raw: &[u8]) -> Result<(), Failure> {
    let encoded = extract_image(raw)?;
    let bytes = STANDARD.decode(encoded).map_err(|_| Failure::new(FailureCode::StreamFailed))?;
    validate_png(&bytes)
}

fn extract_image(raw: &[u8]) -> Result<String, Failure> {
    let text = std::str::from_utf8(raw).map_err(|_| Failure::new(FailureCode::StreamFailed))?;
    let mut scanner = aillies_core::wire::TerminalScanner::default();
    scanner.feed(raw);
    if !scanner.completed {
        return Err(Failure::new(FailureCode::DeliveryUncertain));
    }
    let mut image = None;
    // Join SSE data fields so both CRLF and multiline data are supported.
    let normalized = text.replace("\r\n", "\n");
    for event in normalized.split("\n\n") {
        let data = event.lines().filter_map(|l| l.strip_prefix("data:").map(|s| s.strip_prefix(' ').unwrap_or(s))).collect::<Vec<_>>().join("\n");
        let Ok(value) = serde_json::from_str::<Value>(&data) else { continue };
        match value["type"].as_str() {
            Some("response.failed" | "response.incomplete") => return Err(Failure::new(FailureCode::StreamFailed)),
            Some("response.output_item.done") => {
                let item = &value["item"];
                if item["type"] == "image_generation_call" {
                    image = item["result"].as_str().map(str::to_owned).or(image);
                }
            }
            Some("response.completed") => {
                if let Some(items) = value["response"]["output"].as_array() {
                    for item in items {
                        if item["type"] == "image_generation_call" {
                            image = item["result"].as_str().map(str::to_owned).or(image);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    image.filter(|s| !s.is_empty()).ok_or(Failure::new(FailureCode::StreamFailed))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn png_signature_alone_is_not_a_saved_image() {
        assert!(validate_png(b"\x89PNG\r\n\x1a\n").is_err());
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[0, 0, 255]).unwrap();
        }
        validate_png(&bytes).unwrap();
        assert!(validate_png(&bytes[..bytes.len() - 8]).is_err());
    }
    #[test]
    fn finished_image_requires_a_real_completed_event() {
        let png = "iVBORw0KGgo=";
        let item = format!("data: {{\"type\":\"response.output_item.done\",\"item\":{{\"type\":\"image_generation_call\",\"result\":\"{png}\"}}}}\r\n\r\n");
        assert!(extract_image(item.as_bytes()).is_err());
        let complete = format!("{item}data: {{\"type\":\"response.completed\",\"response\":{{\"status\":\"completed\"}}}}\n\n");
        assert_eq!(extract_image(complete.as_bytes()).unwrap(), png);
        let misleading = format!("{item}data: {{\"type\":\"response.output_text.delta\",\"delta\":\"response.completed\"}}\n\n");
        assert!(extract_image(misleading.as_bytes()).is_err());
    }
    #[test]
    fn upstream_failure_and_text_cannot_become_an_image_or_error_message() {
        assert!(extract_image(b"data: {\"type\":\"response.failed\",\"error\":\"secret\"}\n\n").is_err());
        assert!(extract_image(b"data: {\"type\":\"response.completed\",\"response\":{\"output\":[]}}\n\n").is_err());
    }
}
