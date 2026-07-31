import SwiftUI

struct ConnectionFormView: View {
    @ObservedObject var client = RdpClient.shared
    @Binding var activeTab: Int

    @State private var host: String = "192.168.1.100"
    @State private var port: String = "3389"
    @State private var username: String = "Administrator"
    @State private var password: String = ""
    @State private var isPasswordVisible: Bool = false
    @State private var domain: String = ""
    @State private var selectedProtocol: String = "RDP"
    @State private var selectedResolution: String = "1280x720"

    let resolutions = ["1024x768", "1280x720", "1920x1080", "2560x1440"]

    var body: some View {
        NavigationView {
            Form {
                Section(header: Text("Protocol Selection").font(.headline)) {
                    Picker("Protocol", selection: $selectedProtocol) {
                        Text("Microsoft RDP").tag("RDP")
                        Text("VNC Server").tag("VNC")
                    }
                    .pickerStyle(SegmentedPickerStyle())
                    .onChange(of: selectedProtocol) { newValue in
                        if newValue == "RDP" && port == "5900" {
                            port = "3389"
                        } else if newValue == "VNC" && port == "3389" {
                            port = "5900"
                        }
                    }
                }

                Section(header: Text("Server Credentials")) {
                    HStack {
                        Image(systemName: "network")
                            .foregroundColor(.blue)
                        TextField("Host / IP Address", text: $host)
                            .keyboardType(.numbersAndPunctuation)
                            .autocapitalization(.none)
                    }

                    HStack {
                        Image(systemName: "number")
                            .foregroundColor(.blue)
                        TextField("Port", text: $port)
                            .keyboardType(.numberPad)
                    }

                    HStack {
                        Image(systemName: "person.fill")
                            .foregroundColor(.blue)
                        TextField("Username", text: $username)
                            .autocapitalization(.none)
                    }

                    HStack {
                        Image(systemName: "lock.fill")
                            .foregroundColor(.blue)
                        if isPasswordVisible {
                            TextField("Password", text: $password)
                                .autocapitalization(.none)
                                .disableAutocorrection(true)
                        } else {
                            SecureField("Password", text: $password)
                        }
                        Button(action: {
                            isPasswordVisible.toggle()
                        }) {
                            Image(systemName: isPasswordVisible ? "eye.slash.fill" : "eye.fill")
                                .foregroundColor(.gray)
                        }
                    }

                    if selectedProtocol == "RDP" {
                        HStack {
                            Image(systemName: "building.2.fill")
                                .foregroundColor(.blue)
                            TextField("Domain (Optional)", text: $domain)
                                .autocapitalization(.none)
                        }
                    }
                }

                Section(header: Text("Display Settings")) {
                    Picker("Resolution", selection: $selectedResolution) {
                        ForEach(resolutions, id: \.self) { res in
                            Text(res).tag(res)
                        }
                    }
                }

                Section {
                    Button(action: startConnection) {
                        HStack {
                            Spacer()
                            Image(systemName: "play.fill")
                            Text("Connect Session")
                                .bold()
                            Spacer()
                        }
                        .padding(.vertical, 8)
                        .foregroundColor(.white)
                        .background(Color.blue)
                        .cornerRadius(10)
                    }
                    .buttonStyle(PlainButtonStyle())
                }

                if client.state == .connecting {
                    Section {
                        HStack {
                            ProgressView()
                                .padding(.trailing, 8)
                            Text(client.statusMessage)
                                .font(.footnote)
                                .foregroundColor(.secondary)
                        }
                    }
                } else if client.state == .failed {
                    Section {
                        HStack {
                            Image(systemName: "exclamationmark.triangle.fill")
                                .foregroundColor(.red)
                            Text(client.statusMessage)
                                .font(.footnote)
                                .foregroundColor(.red)
                        }
                    }
                }
            }
            .navigationTitle("Rust RDP / VNC")
        }
    }

    private func startConnection() {
        let portInt = Int32(port) ?? (selectedProtocol == "RDP" ? 3389 : 5900)
        let parts = selectedResolution.split(separator: "x")
        let w = parts.count == 2 ? Int32(parts[0]) ?? 1280 : 1280
        let h = parts.count == 2 ? Int32(parts[1]) ?? 720 : 720

        client.connect(
            host: host,
            port: portInt,
            username: username,
            password: password,
            domain: domain,
            width: w,
            height: h,
            connMode: selectedProtocol
        )

        // Switch to Canvas Tab when connecting
        activeTab = 1
    }
}
