import SwiftUI

struct RemoteCanvasView: View {
    @ObservedObject var client = RdpClient.shared
    @Binding var activeTab: Int

    @State private var scale: CGFloat = 1.0
    @State private var lastScale: CGFloat = 1.0
    @State private var offset: CGSize = .zero
    @State private var lastOffset: CGSize = .zero

    @State private var showKeyboard: Bool = false
    @State private var inputText: String = ""
    @FocusState private var isKeyboardFocused: Bool

    var body: some View {
        ZStack {
            Color.black.edgesIgnoringSafeArea(.all)

            if client.state == .connected, let image = client.remoteImage {
                GeometryReader { geo in
                    Image(uiImage: image)
                        .resizable()
                        .aspectRatio(contentMode: .fit)
                        .scaleEffect(scale)
                        .offset(offset)
                        .gesture(
                            MagnificationGesture()
                                .onChanged { value in
                                    let delta = value / lastScale
                                    lastScale = value
                                    scale = min(max(scale * delta, 0.5), 4.0)
                                }
                                .onEnded { _ in
                                    lastScale = 1.0
                                }
                        )
                        .simultaneousGesture(
                            DragGesture()
                                .onChanged { value in
                                    let remoteCoords = convertToRemoteCoordinates(
                                        location: value.location,
                                        viewSize: geo.size
                                    )
                                    client.sendMouseEvent(x: remoteCoords.x, y: remoteCoords.y, action: 0) // move
                                    offset = CGSize(
                                        width: lastOffset.width + value.translation.width,
                                        height: lastOffset.height + value.translation.height
                                    )
                                }
                                .onEnded { value in
                                    lastOffset = offset
                                }
                        )
                        .simultaneousGesture(
                            TapGesture(count: 1)
                                .onEnded {
                                    // Left click
                                    client.sendMouseEvent(x: Int32(client.remoteWidth / 2), y: Int32(client.remoteHeight / 2), action: 1)
                                    DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) {
                                        client.sendMouseEvent(x: Int32(client.remoteWidth / 2), y: Int32(client.remoteHeight / 2), action: 2)
                                    }
                                }
                        )
                }
            } else {
                VStack(spacing: 16) {
                    Image(systemName: "desktopcomputer")
                        .font(.system(size: 64))
                        .foregroundColor(.gray)

                    Text(client.state == .connecting ? "Connecting Remote Desktop..." : "No Active Session")
                        .font(.headline)
                        .foregroundColor(.white)

                    Text(client.statusMessage)
                        .font(.subheadline)
                        .foregroundColor(.gray)

                    if client.state == .idle || client.state == .failed {
                        Button("Return to Connect Form") {
                            activeTab = 0
                        }
                        .padding()
                        .background(Color.blue)
                        .foregroundColor(.white)
                        .cornerRadius(8)
                    }
                }
            }

            // Floating Quick Action Toolbar
            if client.state == .connected {
                VStack {
                    HStack {
                        Spacer()
                        HStack(spacing: 12) {
                            Button(action: { showKeyboard.toggle(); isKeyboardFocused = showKeyboard }) {
                                Image(systemName: "keyboard")
                                    .padding(10)
                                    .background(Color.black.opacity(0.7))
                                    .foregroundColor(.white)
                                    .clipShape(Circle())
                            }

                            Button(action: sendCtrlAltDel) {
                                Text("CAD")
                                    .font(.caption)
                                    .bold()
                                    .padding(10)
                                    .background(Color.black.opacity(0.7))
                                    .foregroundColor(.yellow)
                                    .clipShape(Circle())
                            }

                            Button(action: { client.disconnect(); activeTab = 0 }) {
                                Image(systemName: "xmark.circle.fill")
                                    .padding(10)
                                    .background(Color.red.opacity(0.8))
                                    .foregroundColor(.white)
                                    .clipShape(Circle())
                            }
                        }
                        .padding(.trailing, 16)
                        .padding(.top, 16)
                    }
                    Spacer()

                    // Hidden Soft Keyboard Receiver
                    if showKeyboard {
                        HStack {
                            TextField("Type text here...", text: $inputText)
                                .focused($isKeyboardFocused)
                                .textFieldStyle(RoundedBorderTextFieldStyle())
                                .onChange(of: inputText) { newValue in
                                    if let lastChar = newValue.last {
                                        let unicodeVal = Int32(lastChar.unicodeScalars.first?.value ?? 0)
                                        client.sendKeyEvent(keycode: unicodeVal, pressed: 1)
                                        client.sendKeyEvent(keycode: unicodeVal, pressed: 0)
                                    }
                                }
                            Button("Done") {
                                showKeyboard = false
                                isKeyboardFocused = false
                            }
                            .padding(.horizontal)
                        }
                        .padding()
                        .background(Color.white)
                    }
                }
            }
        }
    }

    private func convertToRemoteCoordinates(location: CGPoint, viewSize: CGSize) -> (x: Int32, y: Int32) {
        let rw = CGFloat(client.remoteWidth)
        let rh = CGFloat(client.remoteHeight)
        guard viewSize.width > 0, viewSize.height > 0 else { return (0, 0) }

        let scaleFactor = min(viewSize.width / rw, viewSize.height / rh)
        let renderW = rw * scaleFactor
        let renderH = rh * scaleFactor
        let originX = (viewSize.width - renderW) / 2
        let originY = (viewSize.height - renderH) / 2

        let relX = (location.x - originX) / scaleFactor
        let relY = (location.y - originY) / scaleFactor

        let finalX = Int32(min(max(relX, 0), rw - 1))
        let finalY = Int32(min(max(relY, 0), rh - 1))
        return (finalX, finalY)
    }

    private func sendCtrlAltDel() {
        // Ctrl down (scancode 0x1D)
        client.sendScancodeEvent(scancode: 0x1D, isExtended: false, pressed: 1)
        // Alt down (scancode 0x38)
        client.sendScancodeEvent(scancode: 0x38, isExtended: false, pressed: 1)
        // Delete down (scancode 0x53, extended)
        client.sendScancodeEvent(scancode: 0x53, isExtended: true, pressed: 1)

        // Release in reverse
        client.sendScancodeEvent(scancode: 0x53, isExtended: true, pressed: 0)
        client.sendScancodeEvent(scancode: 0x38, isExtended: false, pressed: 0)
        client.sendScancodeEvent(scancode: 0x1D, isExtended: false, pressed: 0)
    }
}
