#!/usr/bin/env swift
// Crops and masks the CodeTake brand artwork (no re-drawing). macOS only.
//
//   swift scripts/brand-image.swift profile <src> <row|col> <index>
//   swift scripts/brand-image.swift crop <src> <dst> <x> <y> <w> <h> [radius] [canvas] [inset]
//   swift scripts/brand-image.swift template <src> <dst>
//
// `crop` cuts the rectangle out of the source. With `radius`, pixels outside
// a rounded rectangle of that corner radius become transparent. With
// `canvas`, the result is centred on a transparent square canvas of that
// size, `inset` pixels from each edge (the macOS icon grid uses 100 of 1024).
//
// `template` turns light artwork on a dark background into a macOS menu bar
// template image: black pixels whose opacity follows the source brightness.

import AppKit
import CoreGraphics

func load(_ path: String) -> CGImage {
    guard let source = CGImageSourceCreateWithURL(URL(fileURLWithPath: path) as CFURL, nil),
          let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
        fatalError("cannot read \(path)")
    }
    return image
}

func rgba(_ image: CGImage) -> (UnsafeMutablePointer<UInt8>, CGContext) {
    let w = image.width, h = image.height
    let data = UnsafeMutablePointer<UInt8>.allocate(capacity: w * h * 4)
    let ctx = CGContext(data: data, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4,
                        space: CGColorSpace(name: CGColorSpace.sRGB)!,
                        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    ctx.draw(image, in: CGRect(x: 0, y: 0, width: w, height: h))
    return (data, ctx)
}

func save(_ image: CGImage, _ path: String) {
    let rep = NSBitmapImageRep(cgImage: image)
    try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: path))
}

let args = CommandLine.arguments
switch args[1] {
case "profile":
    let image = load(args[2])
    let (data, _) = rgba(image)
    let index = Int(args[4])!
    let horizontal = args[3] == "row"
    let count = horizontal ? image.width : image.height
    var out: [String] = []
    for i in stride(from: 0, to: count, by: 1) {
        let (x, y) = horizontal ? (i, index) : (index, i)
        let p = (y * image.width + x) * 4
        let lum = (Int(data[p]) + Int(data[p + 1]) + Int(data[p + 2])) / 3
        out.append("\(i):\(lum)")
    }
    print(out.joined(separator: " "))
case "crop":
    let image = load(args[2])
    let rect = CGRect(x: Double(args[4])!, y: Double(args[5])!, width: Double(args[6])!, height: Double(args[7])!)
    let radius = args.count > 8 ? Double(args[8])! : 0
    guard var cropped = image.cropping(to: rect) else { fatalError("bad crop") }
    if radius > 0 {
        let (_, ctx) = rgba(cropped)
        ctx.clear(CGRect(x: 0, y: 0, width: cropped.width, height: cropped.height))
        let bounds = CGRect(x: 0, y: 0, width: cropped.width, height: cropped.height)
        ctx.addPath(CGPath(roundedRect: bounds, cornerWidth: radius, cornerHeight: radius, transform: nil))
        ctx.clip()
        ctx.draw(cropped, in: bounds)
        cropped = ctx.makeImage()!
    }
    if args.count > 9 {
        let canvas = Int(args[9])!
        let inset = Double(args.count > 10 ? args[10] : "0")!
        let ctx = CGContext(data: nil, width: canvas, height: canvas, bitsPerComponent: 8, bytesPerRow: 0,
                            space: CGColorSpace(name: CGColorSpace.sRGB)!,
                            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        ctx.interpolationQuality = .high
        let side = Double(canvas) - 2 * inset
        let scale = side / Double(max(cropped.width, cropped.height))
        let w = Double(cropped.width) * scale, h = Double(cropped.height) * scale
        ctx.draw(cropped, in: CGRect(x: (Double(canvas) - w) / 2, y: (Double(canvas) - h) / 2, width: w, height: h))
        cropped = ctx.makeImage()!
    }
    save(cropped, args[3])
    print("wrote \(args[3]) (\(cropped.width)x\(cropped.height))")
case "template":
    let image = load(args[2])
    let (data, ctx) = rgba(image)
    for i in 0..<(image.width * image.height) {
        let p = i * 4
        let lum = (0.2126 * Double(data[p]) + 0.7152 * Double(data[p + 1]) + 0.0722 * Double(data[p + 2]))
        // Background is dark (≈20); map 40…230 to fully transparent…opaque.
        let alpha = min(1, max(0, (lum - 40) / 190)) * Double(data[p + 3]) / 255
        data[p] = 0; data[p + 1] = 0; data[p + 2] = 0
        data[p + 3] = UInt8((alpha * 255).rounded())
    }
    save(ctx.makeImage()!, args[3])
    print("wrote \(args[3])")
default:
    print("unknown command")
}
