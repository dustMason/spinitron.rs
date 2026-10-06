import AppKit
import SwiftUI

enum HiFi {
    static let ink = Color(red: 0.08, green: 0.18, blue: 0.29)
    static let secondary = Color(red: 0.25, green: 0.33, blue: 0.41)
    static let edge = Color(red: 0.37, green: 0.42, blue: 0.47)
    static let paper = Color(red: 0.88, green: 0.90, blue: 0.92)
    static let blue = Color(red: 0.04, green: 0.34, blue: 0.56)
    static let highlight = Color(red: 0.68, green: 0.84, blue: 0.97)
    static let lcdTop = Color(red: 0.53, green: 0.76, blue: 0.94)
    static let lcdBottom = Color(red: 0.77, green: 0.89, blue: 0.98)
}

struct SilverSurface: View {
    // Loading explicitly keeps SwiftUI from silently losing the package image
    // when the executable is embedded in the locally assembled .app bundle.
    private static let texture = NSImage(contentsOf:
        Bundle.module.url(forResource: "brushed-silver", withExtension: "png")!)!

    var body: some View {
        HiFi.paper.overlay {
            Image(nsImage: Self.texture)
                .resizable(resizingMode: .tile)
                .opacity(0.85)
        }
        .overlay {
            LinearGradient(colors: [.white.opacity(0.24), .clear, .black.opacity(0.06)],
                           startPoint: .top, endPoint: .bottom)
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

struct HiFiBezel: ViewModifier {
    var radius: CGFloat = 4
    var recessed = false

    func body(content: Content) -> some View {
        content
            .clipShape(RoundedRectangle(cornerRadius: radius))
            .overlay {
                RoundedRectangle(cornerRadius: radius)
                    .strokeBorder(HiFi.edge, lineWidth: 1)
                RoundedRectangle(cornerRadius: max(0, radius - 1))
                    .inset(by: 1)
                    .strokeBorder(
                        LinearGradient(colors: recessed
                            ? [.black.opacity(0.28), .white.opacity(0.8)]
                            : [.white.opacity(0.95), .black.opacity(0.14)],
                            startPoint: .topLeading, endPoint: .bottomTrailing), lineWidth: 1)
            }
    }
}

struct HiFiButtonStyle: ButtonStyle {
    var selected = false
    var compact = false
    @Environment(\.isEnabled) private var enabled
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 12, weight: .semibold, design: .monospaced))
            .foregroundStyle(HiFi.ink)
            .padding(.horizontal, compact ? 8 : 13)
            .frame(minWidth: 30, minHeight: 30)
            .background {
                if selected || configuration.isPressed {
                    LinearGradient(colors: [HiFi.lcdTop, HiFi.lcdBottom],
                                   startPoint: .top, endPoint: .bottom)
                } else {
                    LinearGradient(colors: [.white, HiFi.paper, Color(red: 0.72, green: 0.76, blue: 0.80)],
                                   startPoint: .top, endPoint: .bottom)
                }
            }
            .modifier(HiFiBezel(radius: 3, recessed: configuration.isPressed || selected))
            .offset(y: configuration.isPressed ? 1 : 0)
            .opacity(enabled ? 1 : 0.48)
            .animation(reduceMotion ? nil : .easeOut(duration: 0.09), value: configuration.isPressed)
    }
}

/// A tiny vector dot-matrix display; the accessible value remains ordinary text.
struct StationDisplay: View {
    let station: String
    var subtitle = "BROADCAST CATALOG"

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if station.count <= 6 && station.allSatisfy({ DotMatrixText.glyphs[$0] != nil }) {
                DotMatrixText(text: station, pitch: 3.4)
                    .frame(height: 23.8)
                    .accessibilityLabel(station)
            } else {
                Text(station).font(.system(size: 23, weight: .medium, design: .monospaced))
                    .lineLimit(1).minimumScaleFactor(0.65)
            }
            Text(subtitle)
                .font(.system(size: 9, weight: .semibold, design: .monospaced))
                .tracking(1.1)
        }
        .foregroundStyle(HiFi.ink)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 14).padding(.vertical, 12)
        .background {
            LinearGradient(colors: [HiFi.lcdTop, HiFi.lcdBottom], startPoint: .top, endPoint: .bottom)
            Canvas { context, size in
                // The LCD's cell grid is geometric and independent of the metal texture.
                var cells = Path()
                for x in stride(from: 0.0, to: size.width, by: 3.4) {
                    for y in stride(from: 0.0, to: size.height, by: 3.4) {
                        cells.addRect(CGRect(x: x, y: y, width: 0.5, height: 0.5))
                    }
                }
                context.fill(cells, with: .color(HiFi.blue.opacity(0.16)))
            }.accessibilityHidden(true)
        }
        .modifier(HiFiBezel(radius: 4, recessed: true))
    }
}

struct DotMatrixText: View {
    let text: String
    let pitch: CGFloat

    static let glyphs: [Character: [UInt8]] = [
        "A": [14,17,17,31,17,17,17], "K": [17,18,20,24,20,18,17],
        "L": [16,16,16,16,16,16,31], "X": [17,17,10,4,10,17,17],
        "P": [30,17,17,30,16,16,16], "O": [14,17,17,17,17,17,14],
        "S": [15,16,16,14,1,1,30], "I": [31,4,4,4,4,4,31],
        "N": [17,25,25,21,19,19,17], "T": [31,4,4,4,4,4,4],
        "R": [30,17,17,30,20,18,17],
    ]

    var body: some View {
        Canvas { context, _ in
            var dots = Path()
            for (index, character) in text.enumerated() {
                for (y, row) in (Self.glyphs[character] ?? []).enumerated() {
                    for x in 0..<5 where row & (1 << (4 - x)) != 0 {
                        dots.addRoundedRect(in: CGRect(x: CGFloat(index * 6 + x) * pitch,
                                                       y: CGFloat(y) * pitch,
                                                       width: pitch * 0.77, height: pitch * 0.77),
                                            cornerSize: CGSize(width: pitch * 0.15, height: pitch * 0.15))
                    }
                }
            }
            context.fill(dots, with: .foreground)
        }
        .frame(width: CGFloat(max(0, text.count * 6 - 1)) * pitch, height: 7 * pitch)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(text)
    }
}

struct HiFiRule: View {
    var body: some View {
        VStack(spacing: 0) {
            HiFi.edge.opacity(0.30).frame(height: 1)
            Color.white.opacity(0.65).frame(height: 1)
        }.accessibilityHidden(true)
    }
}
