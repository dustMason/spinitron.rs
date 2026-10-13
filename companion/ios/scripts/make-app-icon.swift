// Reproducible native artwork: the shared Silver Hi-Fi palette and S/P 5x7 glyphs.
// Run from companion/ios: swift scripts/make-app-icon.swift
import AppKit

let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: 1024, pixelsHigh: 1024,
                             bitsPerSample: 8, samplesPerPixel: 3, hasAlpha: false,
                             isPlanar: false, colorSpaceName: .deviceRGB,
                             bytesPerRow: 0, bitsPerPixel: 0)!
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: bitmap)
NSGradient(starting: NSColor(white: 0.97, alpha: 1), ending: NSColor(white: 0.68, alpha: 1))!
    .draw(in: NSRect(x: 0, y: 0, width: 1024, height: 1024), angle: -90)
let frame = NSBezierPath(roundedRect: NSRect(x: 66, y: 108, width: 892, height: 808), xRadius: 42, yRadius: 42)
NSColor(calibratedRed: 0.20, green: 0.27, blue: 0.34, alpha: 1).setFill()
frame.fill()
let lcd = NSBezierPath(roundedRect: NSRect(x: 84, y: 126, width: 856, height: 772), xRadius: 27, yRadius: 27)
NSGradient(starting: NSColor(calibratedRed: 0.53, green: 0.76, blue: 0.94, alpha: 1),
           ending: NSColor(calibratedRed: 0.77, green: 0.89, blue: 0.98, alpha: 1))!.draw(in: lcd, angle: -90)
let glyphs = [[15,16,16,14,1,1,30], [30,17,17,30,16,16,16]]
let pitch: CGFloat = 61
NSColor(calibratedRed: 0.08, green: 0.18, blue: 0.29, alpha: 1).setFill()
for (index, glyph) in glyphs.enumerated() {
    for (y, row) in glyph.enumerated() {
        for x in 0..<5 where row & (1 << (4 - x)) != 0 {
            NSBezierPath(roundedRect: NSRect(x: 184 + CGFloat(index * 6 + x) * pitch,
                                             y: 303 + CGFloat(6 - y) * pitch,
                                             width: pitch * 0.77, height: pitch * 0.77),
                         xRadius: 6, yRadius: 6).fill()
        }
    }
}
NSGraphicsContext.restoreGraphicsState()
let output = URL(fileURLWithPath: "Spinitron/Assets.xcassets/AppIcon.appiconset/AppIcon.png")
try FileManager.default.createDirectory(at: output.deletingLastPathComponent(), withIntermediateDirectories: true)
try bitmap.representation(using: .png, properties: [:])!.write(to: output)
