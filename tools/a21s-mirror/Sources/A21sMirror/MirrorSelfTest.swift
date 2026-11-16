import AppKit
import ImageIO
import UniformTypeIdentifiers

/// Headless check used by `scripts/verify_mirror.sh` and for documentation shots:
/// connects, waits for N frames, writes the last one as PNG, prints timing.
final class MirrorSelfTest: NSObject, RFBClientDelegate {
    private let client: RFBClient
    private let outputPath: String
    private let frameTarget: Int
    private var frames = 0
    private var lastImage: CGImage?
    private var lastFrameAt: Date?
    private let startedAt = Date()
    private var timeoutTimer: Timer?

    private init(host: String, port: UInt16, outputPath: String, frameTarget: Int) {
        self.client = RFBClient(host: host, port: port)
        self.outputPath = outputPath
        self.frameTarget = frameTarget
        super.init()
    }

    static func run(arguments: [String]) -> Never {
        var host = "172.16.42.1"
        var port: UInt16 = 5900
        var output = "/tmp/a21s-mirror.png"
        var frames = 30
        var timeout: TimeInterval = 25

        var iterator = arguments.dropFirst().makeIterator()
        while let argument = iterator.next() {
            switch argument {
            case "--host": host = iterator.next() ?? host
            case "--port": port = UInt16(iterator.next() ?? "") ?? port
            case "--out": output = iterator.next() ?? output
            case "--frames": frames = Int(iterator.next() ?? "") ?? frames
            case "--timeout": timeout = Double(iterator.next() ?? "") ?? timeout
            default: break
            }
        }

        let test = MirrorSelfTest(host: host, port: port, outputPath: output, frameTarget: frames)
        test.start(timeout: timeout)
        RunLoop.main.run()
        exit(0)
    }

    private func start(timeout: TimeInterval) {
        print("selftest: connecting to \(client.host):\(client.port)")
        client.delegate = self
        client.connect()
        timeoutTimer = Timer.scheduledTimer(withTimeInterval: timeout, repeats: false) { [weak self] _ in
            self?.finish(success: false, reason: "timed out")
        }
    }

    func rfbClient(_ client: RFBClient, didConnectWith size: CGSize) {
        print("selftest: connected \(Int(size.width))x\(Int(size.height))")
    }

    func rfbClient(_ client: RFBClient, didUpdate image: CGImage) {
        frames += 1
        lastImage = image
        if let previous = lastFrameAt, frames <= 12 {
            print(String(format: "selftest: frame %d after %.0f ms", frames, Date().timeIntervalSince(previous) * 1000))
        }
        lastFrameAt = Date()
        if frames == 1 {
            print("selftest: first frame \(image.width)x\(image.height)")
        }
        if frames >= frameTarget {
            finish(success: true, reason: "\(frameTarget) frames")
        }
    }

    func rfbClient(_ client: RFBClient, didDisconnect reason: String) {
        finish(success: false, reason: reason)
    }

    private func finish(success: Bool, reason: String) {
        guard timeoutTimer != nil else { return } // already finished
        timeoutTimer?.invalidate()
        timeoutTimer = nil

        let elapsed = Date().timeIntervalSince(startedAt)
        var saved = false
        if let image = lastImage {
            saved = writePNG(image, to: outputPath)
        }
        let fps = elapsed > 0 ? Double(frames) / elapsed : 0
        print(String(
            format: "selftest: %@ — %d frames in %.1fs (%.1f fps), %@",
            success ? "ok" : "failed",
            frames,
            elapsed,
            fps,
            reason
        ))
        if saved {
            print("selftest: wrote \(outputPath)")
        }
        exit(success ? 0 : 1)
    }

    private func writePNG(_ image: CGImage, to path: String) -> Bool {
        let url = URL(fileURLWithPath: path) as CFURL
        guard let destination = CGImageDestinationCreateWithURL(
            url,
            UTType.png.identifier as CFString,
            1,
            nil
        ) else {
            return false
        }
        CGImageDestinationAddImage(destination, image, nil)
        return CGImageDestinationFinalize(destination)
    }
}
