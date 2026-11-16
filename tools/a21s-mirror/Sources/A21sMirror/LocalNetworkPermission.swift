import Foundation
import Network

/// macOS 15+ Local Network privacy: a GUI app that only uses raw BSD sockets can be
/// denied with `EHOSTUNREACH` ("No route to host") *without ever showing the prompt*.
/// Touching the endpoint once through Network.framework makes the system attribute the
/// traffic to this app and raise the permission sheet; the real RFB connection can then
/// use plain sockets as before.
///
/// Command-line launches (e.g. `--selftest` from Terminal) inherit Terminal's exemption,
/// which is why the same binary works there.
enum LocalNetworkPermission {
    private static let lock = NSLock()
    private static var triggered = false

    static func trigger(host: String, port: UInt16) {
        lock.lock()
        guard !triggered else {
            lock.unlock()
            return
        }
        triggered = true
        lock.unlock()

        guard let endpointPort = NWEndpoint.Port(rawValue: port) else { return }
        let connection = NWConnection(
            host: NWEndpoint.Host(host),
            port: endpointPort,
            using: .tcp
        )
        connection.stateUpdateHandler = { state in
            switch state {
            case .ready, .failed, .cancelled:
                connection.cancel()
            default:
                break
            }
        }
        connection.start(queue: .global(qos: .utility))
        DispatchQueue.global(qos: .utility).asyncAfter(deadline: .now() + 5) {
            connection.cancel()
        }
    }
}
