import SwiftUI

struct SavedConnectionsView: View {
    @ObservedObject var client = RdpClient.shared
    @Binding var activeTab: Int

    @State private var profiles: [ConnectionProfile] = [
        ConnectionProfile.defaultRdp,
        ConnectionProfile.defaultVnc
    ]

    var body: some View {
        NavigationView {
            List {
                ForEach(profiles) { profile in
                    HStack {
                        VStack(alignment: .leading, spacing: 4) {
                            HStack {
                                Text(profile.name)
                                    .font(.headline)
                                Spacer()
                                Text(profile.protocolType)
                                    .font(.caption)
                                    .bold()
                                    .padding(.horizontal, 8)
                                    .padding(.vertical, 2)
                                    .background(profile.protocolType == "RDP" ? Color.blue.opacity(0.2) : Color.green.opacity(0.2))
                                    .foregroundColor(profile.protocolType == "RDP" ? .blue : .green)
                                    .cornerRadius(4)
                            }
                            Text("\(profile.host):\(profile.port)")
                                .font(.subheadline)
                                .foregroundColor(.secondary)
                            Text("User: \(profile.username.isEmpty ? "None" : profile.username) • Res: \(profile.width)x\(profile.height)")
                                .font(.caption)
                                .foregroundColor(.gray)
                        }

                        Button(action: { connectProfile(profile) }) {
                            Image(systemName: "play.circle.fill")
                                .font(.title2)
                                .foregroundColor(.blue)
                        }
                        .buttonStyle(BorderlessButtonStyle())
                    }
                    .padding(.vertical, 4)
                }
                .onDelete(perform: deleteProfile)
            }
            .navigationTitle("Bookmarks")
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(action: addSampleProfile) {
                        Image(systemName: "plus")
                    }
                }
            }
        }
    }

    private func connectProfile(_ profile: ConnectionProfile) {
        client.connect(
            host: profile.host,
            port: profile.port,
            username: profile.username,
            password: "",
            domain: profile.domain,
            width: profile.width,
            height: profile.height,
            connMode: profile.protocolType
        )
        activeTab = 1
    }

    private func deleteProfile(at offsets: IndexSet) {
        profiles.remove(atOffsets: offsets)
    }

    private func addSampleProfile() {
        let newP = ConnectionProfile(
            name: "New Connection \(profiles.count + 1)",
            host: "10.0.0.\(100 + profiles.count)",
            port: 3389,
            username: "User",
            domain: "",
            protocolType: "RDP",
            width: 1920,
            height: 1080
        )
        profiles.append(newP)
    }
}
