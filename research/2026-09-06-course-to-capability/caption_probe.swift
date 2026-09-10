// Local macOS research helper. Reads existing videos; never changes source media.
import AVFoundation
import Foundation
import Vision

guard CommandLine.arguments.count == 6,
      let start = Double(CommandLine.arguments[3]),
      let end = Double(CommandLine.arguments[4]),
      let step = Double(CommandLine.arguments[5]),
      start >= 0, end > start, step > 0 else {
    fatalError("usage: caption_probe VIDEO OUTPUT_JSONL START_SECONDS END_SECONDS STEP_SECONDS")
}
let source = URL(fileURLWithPath: CommandLine.arguments[1])
let destination = URL(fileURLWithPath: CommandLine.arguments[2])
guard !FileManager.default.fileExists(atPath: destination.path) else {
    fatalError("Output exists; choose a fresh output path")
}
let generator = AVAssetImageGenerator(asset: AVURLAsset(url: source))
generator.appliesPreferredTrackTransform = true
generator.requestedTimeToleranceBefore = .zero
generator.requestedTimeToleranceAfter = .zero
FileManager.default.createFile(atPath: destination.path, contents: nil)
let output = try FileHandle(forWritingTo: destination)
defer { try? output.close() }
let began = Date()
var count = 0
var failures = 0
for time in stride(from: start, to: end, by: step) {
    try autoreleasepool {
        var row: [String: Any] = ["requested_seconds": time]
        do {
            var actual = CMTime.zero
            let frame = try generator.copyCGImage(at: CMTime(seconds: time, preferredTimescale: 600), actualTime: &actual)
            let request = VNRecognizeTextRequest()
            request.recognitionLevel = .accurate
            request.recognitionLanguages = ["zh-Hans", "en-US"]
            request.usesLanguageCorrection = false
            request.regionOfInterest = CGRect(x: 0, y: 0.015, width: 1, height: 0.22)
            try VNImageRequestHandler(cgImage: frame).perform([request])
            row["actual_seconds"] = actual.seconds
            row["lines"] = (request.results ?? []).compactMap { observation -> [String: Any]? in
                guard let text = observation.topCandidates(1).first else { return nil }
                let box = observation.boundingBox
                return ["text": text.string, "confidence": text.confidence,
                        "box": [box.origin.x, box.origin.y, box.width, box.height]]
            }
        } catch {
            row["error"] = String(describing: error)
            failures += 1
        }
        let encoded = try JSONSerialization.data(withJSONObject: row, options: [.sortedKeys, .withoutEscapingSlashes])
        try output.write(contentsOf: encoded)
        try output.write(contentsOf: Data([10]))
        count += 1
    }
}
let summary: [String: Any] = ["frames": count, "failures": failures,
                              "elapsed_seconds": Date().timeIntervalSince(began),
                              "source": source.path, "output": destination.path,
                              "start": start, "end": end, "step": step,
                              "engine": "Apple Vision VNRecognizeTextRequest accurate zh-Hans en-US; no language correction",
                              "roi_bottom_left_coordinates": [0, 0.015, 1, 0.22],
                              "status": failures == 0 ? "sampled" : "sampled_with_errors"]
let summaryURL = destination.appendingPathExtension("meta.json")
try JSONSerialization.data(withJSONObject: summary, options: [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]).write(to: summaryURL)
print(String(data: try JSONSerialization.data(withJSONObject: summary, options: [.sortedKeys, .withoutEscapingSlashes]), encoding: .utf8)!)
if failures > 0 { exit(1) }
