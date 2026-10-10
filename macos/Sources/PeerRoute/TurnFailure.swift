import Foundation

/// Fixed codes and messages only. Never embed upstream HTTP error bodies, tokens or identifiers.
struct TurnFailure: Error, Codable, LocalizedError {
    enum Code: String, Codable {
        case noAccount = "account_not_selected"
        case invalidAuth = "invalid_auth_file"
        case expiredAuth = "oauth_expired"
        case unavailableAuth = "auth_file_unavailable"
        case changedAccount = "account_changed"
        case rejected = "upstream_rejected"
        case invalidStream = "unexpected_upstream_format"
        case jsonResponse = "unexpected_json_response"
        case failedStream = "stream_failed"
        case unavailable = "upstream_unavailable"
    }
    let code: Code
    var upstreamStatus: Int? = nil
    var errorDescription: String? {
        switch code {
        case .noAccount: return "On the sharing Mac, choose auth.json, then turn sharing off and on."
        case .invalidAuth: return "Choose a valid Codex OAuth auth.json on the sharing Mac. API-key logins are unsupported."
        case .expiredAuth: return "Codex login expired on the sharing Mac. Renew it in Codex, then retry."
        case .unavailableAuth: return "The sharing Mac cannot read its auth.json. Choose the file again there."
        case .changedAccount: return "Codex account changed on the sharing Mac. Choose auth.json again and restart sharing."
        case .rejected:
            switch upstreamStatus {
            case 400: return "OpenAI rejected the request format or model (HTTP 400). Update both Macs and check the selected model."
            case 401: return "OpenAI rejected the sharing Mac’s Codex login (HTTP 401). Renew it in Codex."
            case 403: return "OpenAI denied this request (HTTP 403). Check the sharing account’s access."
            case 404: return "OpenAI could not find the endpoint or model (HTTP 404). Check the model and app version."
            case 429: return "OpenAI rate or usage limit reached (HTTP 429). Wait or check usage on the sharing Mac."
            default: return "OpenAI rejected the turn (HTTP \(upstreamStatus ?? 502))."
            }
        case .invalidStream: return "OpenAI returned an unrecognized response body (HTTP 200); no generation stream was verified."
        case .jsonResponse: return "OpenAI returned JSON without a completed generation (HTTP 200). Try the standard gpt-5.5 model and check the sharing Mac’s login and model access."
        case .failedStream: return "The generation stream failed. No automatic retry was sent."
        case .unavailable: return "The sharing Mac could not reach OpenAI. Check its network."
        }
    }
    struct Envelope: Codable { let error: TurnFailure }
    var json: Data { (try? JSONEncoder().encode(Envelope(error: self))) ?? Data() }
    static func decode(_ data: Data) -> TurnFailure? {
        guard data.count <= 4096, let value = try? JSONDecoder().decode(Envelope.self, from: data),
              value.error.upstreamStatus == nil || (100...599).contains(value.error.upstreamStatus!) else { return nil }
        return value.error
    }
    static func httpMessage(_ status: Int, body: Data = Data()) -> String {
        if let failure = decode(body) { return failure.localizedDescription }
        switch status {
        case 400: return "The sharing Mac rejected the request format (HTTP 400). Update Peer Route on both Macs."
        case 401: return "Pairing access was rejected (HTTP 401). Send a new pairing from the sharing Mac."
        case 404, 407: return "Update Peer Route on the sharing Mac to v0.8.2, then restart sharing (HTTP \(status))."
        case 429: return "The sharing Mac is busy (HTTP 429). Wait for another turn to finish."
        case 503: return TurnFailure(code: .noAccount).localizedDescription
        case 502: return "The sharing Mac could not complete the turn (HTTP 502). Update both Macs to v0.8.2 for the exact cause."
        default: return "Sharing Mac returned HTTP \(status)."
        }
    }
}

struct TurnPeerStatus: Codable {
    let version: Int
    let appVersion: String
    let failure: TurnFailure?
    static let currentVersion = "0.8.3"
}
