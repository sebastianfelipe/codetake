#!/usr/bin/env swift
// Prints the container, tracks, codecs and duration of a recording and
// optionally saves one frame as a PNG. macOS only; no dependencies.
//
//   swift scripts/inspect-recording.swift recording.mp4 [frame.png] [seconds]

import AVFoundation
import AppKit

func fourCC(_ code: FourCharCode) -> String {
    let bytes = [24, 16, 8, 0].map { UInt8((code >> $0) & 0xff) }
    return String(bytes: bytes, encoding: .ascii) ?? "\(code)"
}

let args = CommandLine.arguments
guard args.count >= 2 else {
    print("usage: inspect-recording.swift <file> [frame.png] [seconds]")
    exit(2)
}
let url = URL(fileURLWithPath: args[1])
let asset = AVURLAsset(url: url)
let semaphore = DispatchSemaphore(value: 0)

Task {
    defer { semaphore.signal() }
    do {
        let duration = try await asset.load(.duration)
        let playable = try await asset.load(.isPlayable)
        print("file: \(url.lastPathComponent)")
        print(String(format: "duration: %.3f s, playable: %@", duration.seconds, playable ? "yes" : "no"))
        for track in try await asset.load(.tracks) {
            let formats = try await track.load(.formatDescriptions)
            let range = try await track.load(.timeRange)
            let codec = formats.first.map { fourCC(CMFormatDescriptionGetMediaSubType($0)) } ?? "?"
            var line = "track \(track.trackID): \(track.mediaType.rawValue) \(codec)"
            line += String(format: " start %.3f duration %.3f", range.start.seconds, range.duration.seconds)
            if track.mediaType == .video {
                let size = try await track.load(.naturalSize)
                let fps = try await track.load(.nominalFrameRate)
                let rate = try await track.load(.estimatedDataRate)
                line += String(format: " %.0fx%.0f %.2f fps %.1f Mbps", size.width, size.height, fps, rate / 1_000_000)
            } else if track.mediaType == .audio, let format = formats.first,
                let asbd = CMAudioFormatDescriptionGetStreamBasicDescription(format)?.pointee {
                line += String(format: " %.0f Hz %d ch", asbd.mSampleRate, asbd.mChannelsPerFrame)
            }
            print(line)
        }
        if args.count >= 3 {
            let generator = AVAssetImageGenerator(asset: asset)
            generator.requestedTimeToleranceBefore = .zero
            generator.requestedTimeToleranceAfter = .zero
            let seconds = args.count >= 4 ? Double(args[3]) ?? 1 : min(1, duration.seconds / 2)
            let (image, _) = try await generator.image(at: CMTime(seconds: seconds, preferredTimescale: 600))
            let rep = NSBitmapImageRep(cgImage: image)
            try rep.representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: args[2]))
            print("frame at \(seconds)s saved to \(args[2])")
        }
    } catch {
        print("error: \(error)")
        exit(1)
    }
}
semaphore.wait()
