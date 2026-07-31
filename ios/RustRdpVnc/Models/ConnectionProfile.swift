import Foundation

struct ConnectionProfile: Identifiable, Codable, Equatable {
    var id: UUID = UUID()
    var name: String
    var host: String
    var port: Int32
    var username: String
    var domain: String
    var protocolType: String // "RDP" or "VNC"
    var width: Int32
    var height: Int32

    static let defaultRdp = ConnectionProfile(
        name: "Demo RDP Server",
        host: "192.168.1.100",
        port: 3389,
        username: "Administrator",
        domain: "",
        protocolType: "RDP",
        width: 1280,
        height: 720
    )

    static let defaultVnc = ConnectionProfile(
        name: "Demo VNC Server",
        host: "192.168.1.101",
        port: 5900,
        username: "",
        domain: "",
        protocolType: "VNC",
        width: 1280,
        height: 720
    )

    /// Export to `.rdp` config text format
    func toRdpConfigString() -> String {
        return """
        full address:s:\(host):\(port)
        username:s:\(username)
        domain:s:\(domain)
        desktopwidth:i:\(width)
        desktopheight:i:\(height)
        screen mode id:i:2
        """
    }

    /// Export to `.vnc` config format
    func toVncConfigString() -> String {
        return """
        Host=\(host)
        Port=\(port)
        User=\(username)
        Width=\(width)
        Height=\(height)
        """
    }
}
