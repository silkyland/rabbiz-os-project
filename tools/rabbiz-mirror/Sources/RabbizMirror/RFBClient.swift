import CoreGraphics
import Darwin
import Foundation

enum RFBError: LocalizedError {
    case socket(String)
    case disconnected
    case protocolViolation(String)
    case authentication(String)

    var errorDescription: String? {
        switch self {
        case .socket(let message): return "Connection failed: \(message)"
        case .disconnected: return "The device closed the connection."
        case .protocolViolation(let message): return "VNC protocol error: \(message)"
        case .authentication(let message): return "VNC authentication: \(message)"
        }
    }
}

protocol RFBClientDelegate: AnyObject {
    func rfbClient(_ client: RFBClient, didConnectWith size: CGSize)
    func rfbClient(_ client: RFBClient, didUpdate image: CGImage)
    func rfbClient(_ client: RFBClient, didDisconnect reason: String)
}

/// Minimal RFB 3.8 client for wayvnc: no authentication, ZRLE + CopyRect + Raw,
/// absolute pointer and X11 keysym input.
final class RFBClient {
    weak var delegate: RFBClientDelegate?

    let host: String
    let port: UInt16

    private let framebuffer = Framebuffer()
    private var fd: Int32 = -1
    private var thread: Thread?
    private var shouldStop = false
    private let stateLock = NSLock()
    private let writeLock = NSLock()
    private var inflater: Inflater?

    init(host: String, port: UInt16 = 5900) {
        self.host = host
        self.port = port
    }

    // MARK: - Lifecycle

    func connect() {
        stateLock.lock()
        if thread != nil {
            stateLock.unlock()
            return
        }
        shouldStop = false
        let thread = Thread { [weak self] in self?.run() }
        thread.name = "RabbizMirror.RFB"
        thread.qualityOfService = .userInitiated
        self.thread = thread
        stateLock.unlock()
        thread.start()
    }

    func disconnect() {
        stateLock.lock()
        shouldStop = true
        let socket = fd
        stateLock.unlock()
        if socket >= 0 {
            shutdown(socket, SHUT_RDWR)
        }
    }

    // MARK: - Input

    func sendPointer(x: Int, y: Int, buttons: Int) {
        let clampedX = UInt16(max(0, min(65_535, x)))
        let clampedY = UInt16(max(0, min(65_535, y)))
        let message: [UInt8] = [
            5,
            UInt8(buttons & 0xFF),
            UInt8(clampedX >> 8), UInt8(clampedX & 0xFF),
            UInt8(clampedY >> 8), UInt8(clampedY & 0xFF)
        ]
        send(message)
    }

    func sendKey(keysym: UInt32, down: Bool) {
        let message: [UInt8] = [
            4,
            down ? 1 : 0,
            0, 0,
            UInt8((keysym >> 24) & 0xFF),
            UInt8((keysym >> 16) & 0xFF),
            UInt8((keysym >> 8) & 0xFF),
            UInt8(keysym & 0xFF)
        ]
        send(message)
    }

    // MARK: - Thread body

    private func run() {
        var reason = "Disconnected"
        do {
            let socket = try openSocket()
            setSocket(socket)
            try handshake(socket)
            let size = framebuffer.currentSize()
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                self.delegate?.rfbClient(
                    self,
                    didConnectWith: CGSize(width: size.width, height: size.height)
                )
            }
            try updateLoop(socket)
        } catch let error as RFBError {
            reason = error.errorDescription ?? "Disconnected"
        } catch {
            reason = error.localizedDescription
        }

        let socket = takeSocket()
        if socket >= 0 {
            close(socket)
        }

        stateLock.lock()
        thread = nil
        let stopped = shouldStop
        stateLock.unlock()

        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.delegate?.rfbClient(self, didDisconnect: stopped ? "Disconnected" : reason)
        }
    }

    private var isStopped: Bool {
        stateLock.lock()
        defer { stateLock.unlock() }
        return shouldStop
    }

    private func setSocket(_ socket: Int32) {
        stateLock.lock()
        fd = socket
        stateLock.unlock()
    }

    private func takeSocket() -> Int32 {
        stateLock.lock()
        defer { stateLock.unlock() }
        let socket = fd
        fd = -1
        return socket
    }

    // MARK: - Socket plumbing

    private func openSocket() throws -> Int32 {
        var hints = addrinfo(
            ai_flags: 0,
            ai_family: AF_INET,
            ai_socktype: SOCK_STREAM,
            ai_protocol: IPPROTO_TCP,
            ai_addrlen: 0,
            ai_canonname: nil,
            ai_addr: nil,
            ai_next: nil
        )
        var result: UnsafeMutablePointer<addrinfo>?
        let status = getaddrinfo(host, String(port), &hints, &result)
        guard status == 0, let first = result else {
            throw RFBError.socket(String(cString: gai_strerror(status)))
        }
        defer { freeaddrinfo(result) }

        var socketFD: Int32 = -1
        var lastError = "no usable address"
        var candidate: UnsafeMutablePointer<addrinfo>? = first
        while let address = candidate {
            let fd = Darwin.socket(address.pointee.ai_family, address.pointee.ai_socktype, address.pointee.ai_protocol)
            if fd < 0 {
                lastError = errnoText()
                candidate = address.pointee.ai_next
                continue
            }
            if Darwin.connect(fd, address.pointee.ai_addr, address.pointee.ai_addrlen) == 0 {
                socketFD = fd
                break
            }
            lastError = errnoText()
            close(fd)
            candidate = address.pointee.ai_next
        }
        guard socketFD >= 0 else {
            throw RFBError.socket("\(host):\(port) — \(lastError)")
        }

        var one: Int32 = 1
        setsockopt(socketFD, SOL_SOCKET, SO_NOSIGPIPE, &one, socklen_t(MemoryLayout<Int32>.size))
        setsockopt(socketFD, IPPROTO_TCP, TCP_NODELAY, &one, socklen_t(MemoryLayout<Int32>.size))
        return socketFD
    }

    private func errnoText() -> String {
        String(cString: strerror(errno))
    }

    private func readBytes(_ socket: Int32, _ count: Int) throws -> [UInt8] {
        guard count > 0 else { return [] }
        var buffer = [UInt8](repeating: 0, count: count)
        var received = 0
        while received < count {
            let chunk = buffer.withUnsafeMutableBytes { raw -> Int in
                Darwin.recv(socket, raw.baseAddress! + received, count - received, 0)
            }
            if chunk > 0 {
                received += chunk
            } else if chunk == 0 {
                throw RFBError.disconnected
            } else if errno == EINTR {
                continue
            } else {
                throw RFBError.socket(errnoText())
            }
        }
        return buffer
    }

    private func readUInt8(_ socket: Int32) throws -> UInt8 {
        try readBytes(socket, 1)[0]
    }

    private func readUInt16(_ socket: Int32) throws -> Int {
        let bytes = try readBytes(socket, 2)
        return Int(bytes[0]) << 8 | Int(bytes[1])
    }

    private func readUInt32(_ socket: Int32) throws -> Int {
        let bytes = try readBytes(socket, 4)
        return Int(bytes[0]) << 24 | Int(bytes[1]) << 16 | Int(bytes[2]) << 8 | Int(bytes[3])
    }

    private func write(_ bytes: [UInt8], to socket: Int32) throws {
        var sent = 0
        while sent < bytes.count {
            let chunk = bytes.withUnsafeBytes { raw -> Int in
                Darwin.send(socket, raw.baseAddress! + sent, bytes.count - sent, 0)
            }
            if chunk > 0 {
                sent += chunk
            } else if chunk < 0 && errno == EINTR {
                continue
            } else {
                throw RFBError.socket(errnoText())
            }
        }
    }

    private func send(_ bytes: [UInt8]) {
        writeLock.lock()
        defer { writeLock.unlock() }
        stateLock.lock()
        let socket = fd
        stateLock.unlock()
        guard socket >= 0 else { return }
        try? write(bytes, to: socket)
    }

    // MARK: - Handshake

    private func handshake(_ socket: Int32) throws {
        try write(Array("RFB 003.008\n".utf8), to: socket)

        let version = try readBytes(socket, 12)
        guard String(bytes: version.prefix(3), encoding: .ascii) == "RFB" else {
            throw RFBError.protocolViolation("not a VNC server")
        }
        guard version.count == 12,
              String(bytes: version[0..<7], encoding: .ascii) == "RFB 003",
              version[10] >= UInt8(ascii: "8") else {
            throw RFBError.protocolViolation("server does not speak RFB 3.8")
        }

        let typeCount = Int(try readUInt8(socket))
        if typeCount == 0 {
            throw RFBError.authentication(try readReason(socket) ?? "no security types offered")
        }
        let types = try readBytes(socket, typeCount)
        guard types.contains(1) else {
            throw RFBError.authentication("device requires authentication (types \(types))")
        }

        try write([1], to: socket)
        let result = try readUInt32(socket)
        if result != 0 {
            throw RFBError.authentication(try readReason(socket) ?? "rejected")
        }

        try write([1], to: socket) // shared session
        let header = try readBytes(socket, 24)
        let width = Int(header[0]) << 8 | Int(header[1])
        let height = Int(header[2]) << 8 | Int(header[3])
        let nameLength = Int(header[20]) << 24 | Int(header[21]) << 16
            | Int(header[22]) << 8 | Int(header[23])
        if nameLength > 0 {
            _ = try readBytes(socket, nameLength)
        }
        guard width > 0, height > 0, width <= 16_384, height <= 16_384 else {
            throw RFBError.protocolViolation("implausible framebuffer \(width)x\(height)")
        }
        framebuffer.resize(width: width, height: height)

        try sendSetPixelFormat(socket)
        try sendSetEncodings(socket)

        guard let inflater = Inflater() else {
            throw RFBError.protocolViolation("could not initialise zlib")
        }
        self.inflater = inflater
    }

    private func readReason(_ socket: Int32) throws -> String? {
        let length = try readUInt32(socket)
        guard length > 0, length <= 4_096 else { return nil }
        let bytes = try readBytes(socket, length)
        return String(bytes: bytes, encoding: .utf8)
    }

    private func sendSetPixelFormat(_ socket: Int32) throws {
        var message: [UInt8] = [0, 0, 0, 0] // SetPixelFormat + padding
        message += [32, 24, 0, 1]           // 32 bpp, depth 24, little endian, true colour
        message += [0, 255, 0, 255, 0, 255] // max red, green, blue
        message += [16, 8, 0]               // shifts: red 16, green 8, blue 0
        message += [0, 0, 0]
        try write(message, to: socket)
    }

    private func sendSetEncodings(_ socket: Int32) throws {
        let encodings: [Int32] = [16, 1, -223, 0] // ZRLE, CopyRect, DesktopSize, Raw
        var message: [UInt8] = [2, 0, 0, UInt8(encodings.count)]
        for encoding in encodings {
            let value = UInt32(bitPattern: encoding)
            message += [
                UInt8((value >> 24) & 0xFF),
                UInt8((value >> 16) & 0xFF),
                UInt8((value >> 8) & 0xFF),
                UInt8(value & 0xFF)
            ]
        }
        try write(message, to: socket)
    }

    private func sendFramebufferUpdateRequest(_ socket: Int32, incremental: Bool) throws {
        let size = framebuffer.currentSize()
        let width = UInt16(clamping: size.width)
        let height = UInt16(clamping: size.height)
        let message: [UInt8] = [
            3,
            incremental ? 1 : 0,
            0, 0,
            0, 0,
            UInt8(width >> 8), UInt8(width & 0xFF),
            UInt8(height >> 8), UInt8(height & 0xFF)
        ]
        try write(message, to: socket)
    }

    // MARK: - Update loop

    private func updateLoop(_ socket: Int32) throws {
        var incremental = false
        var lastFullRefresh = Date()
        while !isStopped {
            // wayvnc does not damage on cursor-only moves, so a server-rendered
            // cursor would freeze in the frame. A full repaint every few seconds
            // also heals any damage the compositor dropped.
            let fullRefresh = Date().timeIntervalSince(lastFullRefresh) > 4
            try sendFramebufferUpdateRequest(socket, incremental: incremental && !fullRefresh)
            if fullRefresh {
                lastFullRefresh = Date()
            }
            incremental = true

            let type = try readUInt8(socket)
            switch type {
            case 0:
                let changed = try readFramebufferUpdate(socket)
                if changed {
                    publishFrame()
                } else {
                    Thread.sleep(forTimeInterval: 0.01)
                }
            case 1:
                _ = try readBytes(socket, 1) // padding
                let count = try readUInt16(socket)
                _ = try readBytes(socket, 6 * count)
            case 2:
                break // bell
            case 3:
                _ = try readBytes(socket, 3) // padding
                let length = try readUInt32(socket)
                if length > 0 { _ = try readBytes(socket, length) }
            default:
                throw RFBError.protocolViolation("unknown server message \(type)")
            }
        }
    }

    private func readFramebufferUpdate(_ socket: Int32) throws -> Bool {
        _ = try readBytes(socket, 1) // padding
        let count = try readUInt16(socket)
        var changed = false

        for _ in 0..<count {
            let header = try readBytes(socket, 12)
            let x = Int(header[0]) << 8 | Int(header[1])
            let y = Int(header[2]) << 8 | Int(header[3])
            let width = Int(header[4]) << 8 | Int(header[5])
            let height = Int(header[6]) << 8 | Int(header[7])
            let encoding = Int32(bitPattern: UInt32(header[8]) << 24 | UInt32(header[9]) << 16
                | UInt32(header[10]) << 8 | UInt32(header[11]))

            switch encoding {
            case 16: // ZRLE
                let length = try readUInt32(socket)
                guard length > 0, length <= 64 << 20 else {
                    throw RFBError.protocolViolation("implausible ZRLE block \(length)")
                }
                let compressed = try readBytes(socket, length)
                guard let inflater else {
                    throw RFBError.protocolViolation("ZRLE before handshake")
                }
                let data = try inflater.inflate(compressed, expected: width * height * 3 + 65_536)
                let size = framebuffer.currentSize()
                guard x + width <= size.width, y + height <= size.height else {
                    throw RFBError.protocolViolation("ZRLE rectangle outside framebuffer")
                }
                _ = try framebuffer.withPixels { pointer, stride in
                    try ZRLEDecoder.decode(
                        data,
                        x: x, y: y, width: width, height: height,
                        frame: pointer, frameWidth: stride / 4
                    )
                }
                changed = true

            case 0: // Raw
                let raw = try readBytes(socket, width * height * 4)
                let size = framebuffer.currentSize()
                guard x + width <= size.width, y + height <= size.height else {
                    throw RFBError.protocolViolation("raw rectangle outside framebuffer")
                }
                _ = framebuffer.withPixels { pointer, stride in
                    let frameWidth = stride / 4
                    raw.withUnsafeBytes { source in
                        guard let sourceBase = source.baseAddress else { return }
                        for row in 0..<height {
                            let destination = pointer + ((y + row) * frameWidth + x) * 4
                            memcpy(destination, sourceBase + row * width * 4, width * 4)
                        }
                    }
                }
                changed = true

            case 1: // CopyRect
                let source = try readBytes(socket, 4)
                let sourceX = Int(source[0]) << 8 | Int(source[1])
                let sourceY = Int(source[2]) << 8 | Int(source[3])
                let size = framebuffer.currentSize()
                guard sourceX + width <= size.width, sourceY + height <= size.height,
                      x + width <= size.width, y + height <= size.height else {
                    throw RFBError.protocolViolation("CopyRect outside framebuffer")
                }
                _ = framebuffer.withPixels { pointer, stride in
                    let frameWidth = stride / 4
                    let rowBytes = width * 4
                    let forward = sourceY < y || (sourceY == y && sourceX < x)
                    let rows = forward ? Array(0..<height) : Array((0..<height).reversed())
                    for row in rows {
                        let sourceOffset = ((sourceY + row) * frameWidth + sourceX) * 4
                        let destinationOffset = ((y + row) * frameWidth + x) * 4
                        memmove(pointer + destinationOffset, pointer + sourceOffset, rowBytes)
                    }
                }
                changed = true

            case -223: // DesktopSize
                framebuffer.resize(width: width, height: height)
                changed = true

            case -239: // Cursor (not requested, but tolerate it)
                _ = try readBytes(socket, width * height * 4 + ((width + 7) / 8) * height)

            default:
                throw RFBError.protocolViolation("unsupported encoding \(encoding)")
            }
        }
        return changed
    }

    private func publishFrame() {
        guard let image = framebuffer.snapshot() else { return }
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.delegate?.rfbClient(self, didUpdate: image)
        }
    }
}
