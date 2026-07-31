import SwiftUI

struct ContentView: View {
    @State private var activeTab: Int = 0
    @ObservedObject var client = RdpClient.shared

    var body: some View {
        TabView(selection: $activeTab) {
            ConnectionFormView(activeTab: $activeTab)
                .tabItem {
                    Label("Connect", systemImage: "bolt.horizontal.fill")
                }
                .tag(0)

            RemoteCanvasView(activeTab: $activeTab)
                .tabItem {
                    Label("Remote View", systemImage: "desktopcomputer")
                }
                .tag(1)

            SavedConnectionsView(activeTab: $activeTab)
                .tabItem {
                    Label("Bookmarks", systemImage: "bookmark.fill")
                }
                .tag(2)

            SettingsView()
                .tabItem {
                    Label("Settings", systemImage: "gearshape.fill")
                }
                .tag(3)
        }
        .accentColor(.blue)
    }
}
