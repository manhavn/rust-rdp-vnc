import SwiftUI

struct SettingsView: View {
    @AppStorage("touchMode") private var touchMode: String = "Direct Touch"
    @AppStorage("mouseSensitivity") private var mouseSensitivity: Double = 1.0
    @AppStorage("autoReconnect") private var autoReconnect: Bool = true
    @AppStorage("enableHaptics") private var enableHaptics: Bool = true

    var body: some View {
        NavigationView {
            Form {
                Section(header: Text("Interaction Preferences")) {
                    Picker("Touch Mode", selection: $touchMode) {
                        Text("Direct Touch").tag("Direct Touch")
                        Text("Virtual Trackpad").tag("Virtual Trackpad")
                    }

                    VStack(alignment: .leading) {
                        HStack {
                            Text("Mouse Sensitivity")
                            Spacer()
                            Text(String(format: "%.1fx", mouseSensitivity))
                                .foregroundColor(.secondary)
                        }
                        Slider(value: $mouseSensitivity, in: 0.5...3.0, step: 0.1)
                    }

                    Toggle("Enable Haptic Feedback", isOn: $enableHaptics)
                }

                Section(header: Text("Network & Session")) {
                    Toggle("Auto Reconnect on Drop", isOn: $autoReconnect)
                }

                Section(header: Text("About Rust RDP VNC")) {
                    HStack {
                        Text("App Version")
                        Spacer()
                        Text("1.0.5")
                            .foregroundColor(.secondary)
                    }

                    HStack {
                        Text("Rust Core Engine")
                        Spacer()
                        Text("IronRDP + vnc-rs")
                            .foregroundColor(.secondary)
                    }

                    HStack {
                        Text("Target Architecture")
                        Spacer()
                        Text("arm64 (iOS)")
                            .foregroundColor(.secondary)
                    }
                }
            }
            .navigationTitle("Settings")
        }
    }
}
