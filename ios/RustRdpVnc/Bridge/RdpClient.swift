import Foundation
import UIKit
import SwiftUI
import Combine

enum SessionState: Int, CustomStringConvertible {
    case idle = 0
    case connecting = 1
    case connected = 2
    case failed = 3

    var description: String {
        switch self {
        case .idle: return "Disconnected"
        case .connecting: return "Connecting..."
        case .connected: return "Connected"
        case .failed: return "Connection Failed"
        }
    }
}

final class RdpClient: ObservableObject {
    static let shared = RdpClient()

    @Published var state: SessionState = .idle
    @Published var statusMessage: String = "Ready"
    @Published var remoteImage: UIImage? = nil
    @Published var remoteWidth: Int = 1280
    @Published var remoteHeight: Int = 720
    @Published var activeSessionId: UInt64 = 0

    private var pixelBuffer: [UInt32] = []
    private let queue = DispatchQueue(label: "com.rustrdp.framequeue", qos: .userInteractive)

    init() {
        rust_rdp_init()
    }

    func connect(
        host: String,
        port: Int32,
        username: String,
        password: String,
        domain: String = "",
        width: Int32 = 1280,
        height: Int32 = 720,
        connMode: String = "RDP"
    ) {
        self.remoteWidth = Int(width)
        self.remoteHeight = Int(height)
        self.pixelBuffer = Array(repeating: 0xFF000000, count: Int(width * height))
        self.state = .connecting
        self.statusMessage = "Connecting to \(host)..."

        let selfPtr = Unmanaged.passUnretained(self).toOpaque()

        let stateCb: StateChangedCallback = { user_data, state, msgPtr in
            guard let user_data = user_data else { return }
            let client = Unmanaged<RdpClient>.fromOpaque(user_data).takeUnretainedValue()
            let msg = msgPtr != nil ? String(cString: msgPtr!) : ""
            DispatchQueue.main.async {
                client.state = SessionState(rawValue: Int(state)) ?? .idle
                client.statusMessage = msg
            }
        }

        let frameCb: FrameDecodedCallback = { user_data, pixelsPtr, len, x, y, width, height in
            guard let user_data = user_data, let pixelsPtr = pixelsPtr else { return }
            let client = Unmanaged<RdpClient>.fromOpaque(user_data).takeUnretainedValue()
            let pixels = UnsafeBufferPointer(start: pixelsPtr, count: Int(len))

            client.queue.async {
                client.updateFrameBuffer(pixels: Array(pixels), x: Int(x), y: Int(y), width: Int(width), height: Int(height))
            }
        }

        let resCb: ResolutionChangedCallback = { user_data, width, height in
            guard let user_data = user_data else { return }
            let client = Unmanaged<RdpClient>.fromOpaque(user_data).takeUnretainedValue()
            DispatchQueue.main.async {
                client.remoteWidth = Int(width)
                client.remoteHeight = Int(height)
                client.pixelBuffer = Array(repeating: 0xFF000000, count: Int(width * height))
            }
        }

        let sessionId = rust_rdp_connect(
            (host as NSString).utf8String,
            port,
            (username as NSString).utf8String,
            (password as NSString).utf8String,
            (domain as NSString).utf8String,
            width,
            height,
            (connMode as NSString).utf8String,
            selfPtr,
            stateCb,
            frameCb,
            resCb
        )
        self.activeSessionId = sessionId
    }

    func disconnect() {
        rust_rdp_disconnect()
        DispatchQueue.main.async {
            self.state = .idle
            self.statusMessage = "Disconnected"
            self.remoteImage = nil
            self.activeSessionId = 0
        }
    }

    // Input APIs
    func sendMouseEvent(x: Int32, y: Int32, action: Int32) {
        rust_rdp_send_mouse_event(x, y, action)
    }

    func sendMouseWheelEvent(x: Int32, y: Int32, units: Int32) {
        rust_rdp_send_mouse_wheel_event(x, y, units)
    }

    func sendMouseHorizontalWheelEvent(x: Int32, y: Int32, units: Int32) {
        rust_rdp_send_mouse_horizontal_wheel_event(x, y, units)
    }

    func sendKeyEvent(keycode: Int32, pressed: Int32) {
        rust_rdp_send_key_event(keycode, pressed)
    }

    func sendScancodeEvent(scancode: Int32, isExtended: Bool, pressed: Int32) {
        rust_rdp_send_scancode_event(scancode, isExtended ? 1 : 0, pressed)
    }

    private func updateFrameBuffer(pixels: [Int32], x: Int, y: Int, width: Int, height: Int) {
        guard width > 0, height > 0 else { return }

        // Copy incoming ARGB/RGBA pixels into destination buffer
        if x == 0 && y == 0 && width == remoteWidth && height == remoteHeight {
            pixelBuffer = pixels.map { UInt32(bitPattern: $0) }
        } else {
            let totalLen = remoteWidth * remoteHeight
            for row in 0..<height {
                let srcStart = row * width
                let dstStart = (y + row) * remoteWidth + x
                for col in 0..<width {
                    if srcStart + col < pixels.count && dstStart + col < totalLen {
                        pixelBuffer[dstStart + col] = UInt32(bitPattern: pixels[srcStart + col])
                    }
                }
            }
        }

        if let image = renderUIImage(pixels: pixelBuffer, width: remoteWidth, height: remoteHeight) {
            DispatchQueue.main.async {
                self.remoteImage = image
            }
        }
    }

    private func renderUIImage(pixels: [UInt32], width: Int, height: Int) -> UIImage? {
        var rawPixels = pixels
        let data = Data(bytes: &rawPixels, count: width * height * 4)
        guard let provider = CGDataProvider(data: data as CFData) else { return nil }

        let colorSpace = CGColorSpaceCreateDeviceRGB()
        let bitmapInfo: CGBitmapInfo = [
            CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedFirst.rawValue),
            CGBitmapInfo(rawValue: CGImageByteOrderInfo.order32Little.rawValue)
        ]

        guard let cgImage = CGImage(
            width: width,
            height: height,
            bitsPerComponent: 8,
            bitsPerPixel: 32,
            bytesPerRow: width * 4,
            space: colorSpace,
            bitmapInfo: bitmapInfo,
            provider: provider,
            decode: nil,
            shouldInterpolate: false,
            intent: .defaultIntent
        ) else { return nil }

        return UIImage(cgImage: cgImage)
    }
}
