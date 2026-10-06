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

/// The socket stays still while the key face travels inside it.
struct HiFiButtonSurface: ViewModifier {
    var selected = false
    var pressed = false
    @Environment(\.isEnabled) private var enabled
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var hovered = false

    private var inset: Bool { selected || pressed }

    func body(content: Content) -> some View {
        content
            .offset(y: pressed ? 1 : 0)
            .background {
                RoundedRectangle(cornerRadius: 3)
                    .fill(Color(red: 0.30, green: 0.36, blue: 0.42))
                    .overlay {
                        RoundedRectangle(cornerRadius: 3)
                            .strokeBorder(LinearGradient(colors: [.black.opacity(0.18), .white.opacity(0.9)],
                                                         startPoint: .top, endPoint: .bottom), lineWidth: 1)
                    }
                RoundedRectangle(cornerRadius: 2)
                    .fill(LinearGradient(stops: inset ? [
                        .init(color: HiFi.lcdTop.opacity(0.88), location: 0),
                        .init(color: HiFi.lcdBottom, location: 1),
                    ] : [
                        .init(color: .white, location: 0),
                        .init(color: Color(red: 0.91, green: 0.93, blue: 0.95), location: 0.42),
                        .init(color: Color(red: 0.80, green: 0.83, blue: 0.87), location: 0.48),
                        .init(color: Color(red: 0.89, green: 0.91, blue: 0.94), location: 1),
                    ], startPoint: .top, endPoint: .bottom))
                    .overlay {
                        RoundedRectangle(cornerRadius: 2)
                            .strokeBorder(LinearGradient(colors: inset
                                ? [HiFi.ink.opacity(0.4), .white.opacity(0.6)]
                                : [.white, HiFi.edge.opacity(0.45)],
                                startPoint: .top, endPoint: .bottom), lineWidth: 1)
                    }
                    .overlay {
                        if hovered && enabled && !pressed {
                            RoundedRectangle(cornerRadius: 2)
                                .fill(.white.opacity(0.15))
                        }
                    }
                    .padding(.horizontal, 1)
                    .padding(.top, inset ? 2 : 1)
                    .padding(.bottom, inset ? 1 : 3)
            }
            .overlay(alignment: .bottom) {
                if selected {
                    Capsule()
                        .fill(HiFi.blue)
                        .frame(width: 14, height: 2)
                        .overlay(alignment: .top) { Color.white.opacity(0.7).frame(height: 0.5) }
                        .padding(.bottom, 4)
                        .allowsHitTesting(false)
                }
            }
            .overlay {
                if hovered && enabled {
                    RoundedRectangle(cornerRadius: 3)
                        .strokeBorder(HiFi.blue.opacity(0.75), lineWidth: 1)
                        .allowsHitTesting(false)
                }
            }
            .shadow(color: .black.opacity(inset ? 0.04 : 0.12), radius: 0.6, x: 0, y: 1)
            .contentShape(RoundedRectangle(cornerRadius: 3))
            .opacity(enabled ? 1 : 0.48)
            .onHover { hovered = $0 }
            .animation(reduceMotion ? nil : .easeOut(duration: 0.09), value: pressed)
    }
}

struct HiFiButtonStyle: ButtonStyle {
    var selected = false
    var compact = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 12, weight: .semibold, design: .monospaced))
            .foregroundStyle(HiFi.ink)
            .shadow(color: .white.opacity(0.75), radius: 0, x: 0, y: 0.5)
            .padding(.horizontal, compact ? 8 : 13)
            .frame(minWidth: 30, minHeight: 30)
            .modifier(HiFiButtonSurface(selected: selected, pressed: configuration.isPressed))
    }
}

/// A tiny vector dot-matrix display; the accessible value remains ordinary text.
struct StationDisplay: View {
    let station: String
    var subtitle = "BROADCAST CATALOG"

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if station.count <= 6 && station.allSatisfy({ DotMatrixText.glyphs[$0] != nil }) {
                DotMatrixText(text: station, pitch: 3.4, showsInactiveCells: true)
                    .shadow(color: HiFi.ink.opacity(0.18), radius: 0.5, x: 0, y: 0.7)
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
        // Reflection and edge shading sit over the pixels, like the glass over an LCD.
        .overlay {
            LinearGradient(stops: [
                .init(color: .white.opacity(0.20), location: 0),
                .init(color: .white.opacity(0.05), location: 0.45),
                .init(color: .clear, location: 0.46),
                .init(color: .white.opacity(0.06), location: 1),
            ], startPoint: .topLeading, endPoint: .bottomTrailing)
            VStack(spacing: 0) {
                LinearGradient(colors: [HiFi.ink.opacity(0.42), .clear], startPoint: .top, endPoint: .bottom)
                    .frame(height: 9)
                Spacer(minLength: 0)
                LinearGradient(colors: [.clear, .white.opacity(0.3)], startPoint: .top, endPoint: .bottom)
                    .frame(height: 5)
            }.padding(3)
        }
        .clipShape(RoundedRectangle(cornerRadius: 4))
        .overlay {
            RoundedRectangle(cornerRadius: 4)
                .strokeBorder(LinearGradient(colors: [HiFi.edge, .white.opacity(0.95)],
                                             startPoint: .top, endPoint: .bottom), lineWidth: 1)
            RoundedRectangle(cornerRadius: 3).inset(by: 1)
                .strokeBorder(Color(red: 0.20, green: 0.27, blue: 0.34), lineWidth: 2)
            RoundedRectangle(cornerRadius: 1).inset(by: 3)
                .strokeBorder(LinearGradient(colors: [HiFi.ink.opacity(0.35), .white.opacity(0.65)],
                                             startPoint: .top, endPoint: .bottom), lineWidth: 0.5)
        }
        .allowsHitTesting(false)
    }
}

struct DotMatrixText: View {
    let text: String
    let pitch: CGFloat
    var showsInactiveCells = false

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
            var inactive = Path()
            for (index, character) in text.enumerated() {
                for (y, row) in (Self.glyphs[character] ?? []).enumerated() {
                    for x in 0..<5 {
                        let cell = Path(roundedRect: CGRect(x: CGFloat(index * 6 + x) * pitch,
                                                           y: CGFloat(y) * pitch,
                                                           width: pitch * 0.77, height: pitch * 0.77),
                                        cornerRadius: pitch * 0.15)
                        if row & (1 << (4 - x)) != 0 {
                            dots.addPath(cell)
                        } else if showsInactiveCells {
                            inactive.addPath(cell)
                        }
                    }
                }
            }
            context.fill(inactive, with: .color(HiFi.ink.opacity(0.08)))
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
