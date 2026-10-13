#if os(macOS)
import AppKit
#else
import UIKit
#endif
import SwiftUI

public enum HiFi {
    public static let ink = Color(red: 0.08, green: 0.18, blue: 0.29)
    public static let secondary = Color(red: 0.25, green: 0.33, blue: 0.41)
    public static let edge = Color(red: 0.37, green: 0.42, blue: 0.47)
    public static let paper = Color(red: 0.88, green: 0.90, blue: 0.92)
    public static let blue = Color(red: 0.04, green: 0.34, blue: 0.56)
    public static let highlight = Color(red: 0.68, green: 0.84, blue: 0.97)
    public static let lcdTop = Color(red: 0.53, green: 0.76, blue: 0.94)
    public static let lcdBottom = Color(red: 0.77, green: 0.89, blue: 0.98)
    public static let stationControlHeight: CGFloat = 64
}

public struct SilverSurface: View {
    public init() {}

    // Loading explicitly keeps SwiftUI from silently losing the package image
    // when the executable is embedded in the locally assembled .app bundle.
    private static let textureURL = Bundle.module.url(forResource: "brushed-silver", withExtension: "png")!
    #if os(macOS)
    private static let texture = NSImage(contentsOf: textureURL)!
    private var textureImage: Image { Image(nsImage: Self.texture) }
    #else
    private static let texture = UIImage(contentsOfFile: textureURL.path)!
    private var textureImage: Image { Image(uiImage: Self.texture) }
    #endif

    public var body: some View {
        HiFi.paper.overlay {
            textureImage
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

public struct HiFiBezel: ViewModifier {
    public init(radius: CGFloat = 4, recessed: Bool = false) { self.radius = radius; self.recessed = recessed }

    var radius: CGFloat = 4
    var recessed = false

    public func body(content: Content) -> some View {
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
public struct HiFiButtonSurface: ViewModifier {
    public init(pressed: Bool = false) { self.pressed = pressed }

    var pressed = false
    @Environment(\.isEnabled) private var enabled
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var hovered = false

    public func body(content: Content) -> some View {
        content
            .offset(y: pressed ? 1 : 0)
            .background {
                // One continuous rim: layered bottom strokes made the inset key
                // look like it had a loose white lip beneath the face.
                RoundedRectangle(cornerRadius: 3)
                    .fill(HiFi.edge)
                RoundedRectangle(cornerRadius: 2)
                    .fill(LinearGradient(stops: pressed ? [
                        .init(color: HiFi.lcdTop, location: 0),
                        .init(color: HiFi.lcdBottom, location: 1),
                    ] : [
                        .init(color: .white, location: 0),
                        .init(color: Color(red: 0.91, green: 0.93, blue: 0.95), location: 0.42),
                        .init(color: Color(red: 0.80, green: 0.83, blue: 0.87), location: 0.48),
                        .init(color: Color(red: 0.89, green: 0.91, blue: 0.94), location: 1),
                    ], startPoint: .top, endPoint: .bottom))
                    .overlay {
                        RoundedRectangle(cornerRadius: 2)
                            .strokeBorder(LinearGradient(colors: pressed
                                ? [HiFi.ink.opacity(0.4), .clear]
                                : [.white.opacity(0.85), .clear],
                                startPoint: .top, endPoint: .bottom), lineWidth: 0.5)
                    }
                    .overlay {
                        if hovered && enabled && !pressed {
                            RoundedRectangle(cornerRadius: 2)
                                .fill(.white.opacity(0.15))
                        }
                    }
                    .padding(.horizontal, 1)
                    .padding(.top, pressed ? 2 : 1)
                    .padding(.bottom, 1)
            }
            .overlay {
                if hovered && enabled {
                    RoundedRectangle(cornerRadius: 3)
                        .strokeBorder(HiFi.blue.opacity(0.75), lineWidth: 1)
                        .allowsHitTesting(false)
                }
            }
            .contentShape(RoundedRectangle(cornerRadius: 3))
            .opacity(enabled ? 1 : 0.48)
            .onHover { hovered = $0 }
            .animation(reduceMotion ? nil : .easeOut(duration: 0.09), value: pressed)
    }
}

public struct HiFiButtonStyle: ButtonStyle {
    public init(compact: Bool = false, touch: Bool = false) { self.compact = compact; self.touch = touch }
    var touch = false

    var compact = false

    public func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(touch ? .system(.body, design: .monospaced).weight(.semibold) : .system(size: 12, weight: .semibold, design: .monospaced))
            .foregroundStyle(HiFi.ink)
            .shadow(color: .white.opacity(0.75), radius: 0, x: 0, y: 0.5)
            .padding(.horizontal, compact ? 8 : 13)
            .frame(minWidth: touch ? 44 : 30, minHeight: touch ? 44 : 30)
            .frame(height: touch ? nil : 30)
            .modifier(HiFiButtonSurface(pressed: configuration.isPressed))
    }
}

/// A tiny vector dot-matrix display; the accessible value remains ordinary text.
public struct StationDisplay: View {
    public init(station: String, height: CGFloat = HiFi.stationControlHeight) {
        self.station = station
        self.height = height
    }

    let station: String
    let height: CGFloat

    public var body: some View {
        GeometryReader { geometry in
            let displayName = station.uppercased()
            let columns = CGFloat(max(1, displayName.count * 6 - 1))
            let pitch = 0.85 * min(geometry.size.height / 7, geometry.size.width / columns)
            DotMatrixText(text: displayName, pitch: pitch, showsInactiveCells: true)
                .frame(maxHeight: .infinity)
                .accessibilityLabel(station)
        }
        .shadow(color: HiFi.ink.opacity(0.18), radius: 0.5, x: 0, y: 0.7)
        .foregroundStyle(HiFi.ink)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .frame(height: height)
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

public struct DotMatrixText: View {
    public init(text: String, pitch: CGFloat, showsInactiveCells: Bool = false) { self.text = text; self.pitch = pitch; self.showsInactiveCells = showsInactiveCells }

    let text: String
    let pitch: CGFloat
    var showsInactiveCells = false

    static let glyphs: [Character: [UInt8]] = [
        "A": [14,17,17,31,17,17,17], "B": [30,17,17,30,17,17,30],
        "C": [14,17,16,16,16,17,14], "D": [30,17,17,17,17,17,30],
        "E": [31,16,16,30,16,16,31], "F": [31,16,16,30,16,16,16],
        "G": [14,17,16,23,17,17,15], "H": [17,17,17,31,17,17,17],
        "I": [31,4,4,4,4,4,31], "J": [7,2,2,2,18,18,12],
        "K": [17,18,20,24,20,18,17], "L": [16,16,16,16,16,16,31],
        "M": [17,27,21,21,17,17,17], "N": [17,25,25,21,19,19,17],
        "O": [14,17,17,17,17,17,14], "P": [30,17,17,30,16,16,16],
        "Q": [14,17,17,17,21,18,13],
        "R": [30,17,17,30,20,18,17],
        "S": [15,16,16,14,1,1,30], "T": [31,4,4,4,4,4,4],
        "U": [17,17,17,17,17,17,14], "V": [17,17,17,17,17,10,4],
        "W": [17,17,17,21,21,27,17], "X": [17,17,10,4,10,17,17],
        "Y": [17,17,10,4,4,4,4], "Z": [31,1,2,4,8,16,31],
        "0": [14,17,19,21,25,17,14], "1": [4,12,4,4,4,4,14],
        "2": [14,17,1,2,4,8,31], "3": [30,1,1,14,1,1,30],
        "4": [2,6,10,18,31,2,2], "5": [31,16,16,30,1,1,30],
        "6": [14,16,16,30,17,17,14], "7": [31,1,2,4,8,8,8],
        "8": [14,17,17,14,17,17,14], "9": [14,17,17,15,1,1,14],
        " ": [0,0,0,0,0,0,0], "-": [0,0,0,31,0,0,0],
        ".": [0,0,0,0,0,12,12], "&": [12,18,20,8,21,18,13],
        "'": [4,4,8,0,0,0,0], "/": [1,2,2,4,8,8,16],
        "?": [14,17,1,2,4,0,4],
    ]

    public var body: some View {
        Canvas { context, _ in
            var dots = Path()
            var inactive = Path()
            for (index, character) in text.enumerated() {
                for (y, row) in (Self.glyphs[character] ?? Self.glyphs["?"]!).enumerated() {
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

public struct HiFiRule: View {
    public init() {}

    public var body: some View {
        VStack(spacing: 0) {
            HiFi.edge.opacity(0.30).frame(height: 1)
            Color.white.opacity(0.65).frame(height: 1)
        }.accessibilityHidden(true)
    }
}
