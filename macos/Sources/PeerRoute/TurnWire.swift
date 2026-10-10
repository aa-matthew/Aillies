import Foundation

/// Identify the body rather than treating a MIME header as proof of stream format.
enum TurnWire {
    enum Format { case pending, sse, json, other }
    static func detect(_ data: Data) -> Format {
        let bom: [UInt8] = [0xef, 0xbb, 0xbf]
        if !data.isEmpty, data.count < bom.count, Array(data) == Array(bom.prefix(data.count)) { return .pending }
        var text = String(decoding: data, as: UTF8.self)
        if text.hasPrefix("\u{feff}") { text.removeFirst() }
        text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if text.isEmpty { return .pending }
        if text.hasPrefix("event:") || text.hasPrefix("data:") || text.hasPrefix(":") { return .sse }
        if text.hasPrefix("{") || text.hasPrefix("[") { return .json }
        if ["event:", "data:"].contains(where: { $0.hasPrefix(text) }) { return .pending }
        return .other
    }
    /// A completed Responses JSON document can be represented as a finite SSE
    /// sequence. These are buffered results, never invented incremental tokens.
    static func completedJSON(_ data: Data) throws -> [Data] {
        guard let response = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              response["object"] as? String == "response", response["status"] as? String == "completed",
              let output = response["output"] as? [[String: Any]] else { throw TurnFailure(code: .invalidStream, upstreamStatus: 200) }
        func event(_ type: String, _ value: [String: Any]) throws -> Data {
            Data("event: \(type)\ndata: ".utf8) + (try JSONSerialization.data(withJSONObject: value)) + Data("\n\n".utf8)
        }
        var events: [Data] = []
        for (index, item) in output.enumerated() {
            // Preserve completed output exactly; don't claim it arrived token by token.
            events.append(try event("response.output_item.done", ["type": "response.output_item.done", "output_index": index, "item": item]))
        }
        events.append(try event("response.completed", ["type": "response.completed", "response": response]))
        return events
    }
}
