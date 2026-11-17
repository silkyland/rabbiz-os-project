import AppKit

final class MainWindowController: NSWindowController, RFBClientDelegate, NSWindowDelegate, NSTextFieldDelegate {
    private let mirrorView = MirrorView()
    private let hostField = NSTextField(string: "172.16.42.1")
    private let connectButton = NSButton(title: "Connect", target: nil, action: nil)
    private let startButton = NSButton(title: "", target: nil, action: nil)
    private let fullscreenButton = NSButton(title: "", target: nil, action: nil)
    private let statusDot = NSView()
    private let statusLabel = NSTextField(labelWithString: "Not connected")
    private let statsLabel = NSTextField(labelWithString: "—")

    private var client: RFBClient?
    private var connected = false
    private var currentSize = CGSize.zero
    private var framesThisSecond = 0
    private var fpsTimer: Timer?
    private var userStopped = false
    private var retryTimer: Timer?
    private var retryAttempts = 0

    init() {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 400, height: 900),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        window.title = "Rabbiz Mirror"
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.isMovableByWindowBackground = false
        window.minSize = NSSize(width: 380, height: 560)
        window.center()
        super.init(window: window)
        window.delegate = self
        buildInterface()
        startFPSClock()
        if let screen = window.screen {
            var frame = window.frame
            frame.size.height = min(frame.height, screen.visibleFrame.height - 24)
            frame.size.width = min(frame.width, screen.visibleFrame.width - 24)
            window.setFrame(frame, display: false)
        }
        window.center()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) is not used")
    }

    deinit {
        fpsTimer?.invalidate()
    }

    // MARK: - Interface

    private func buildInterface() {
        guard let window, let root = window.contentView else { return }
        root.wantsLayer = true
        root.layer?.backgroundColor = NSColor.windowBackgroundColor.cgColor

        let topBar = NSVisualEffectView()
        topBar.material = .headerView
        topBar.blendingMode = .withinWindow
        topBar.state = .active
        topBar.translatesAutoresizingMaskIntoConstraints = false

        let bottomBar = NSVisualEffectView()
        bottomBar.material = .headerView
        bottomBar.blendingMode = .withinWindow
        bottomBar.state = .active
        bottomBar.translatesAutoresizingMaskIntoConstraints = false

        mirrorView.translatesAutoresizingMaskIntoConstraints = false
        mirrorView.placeholderText = "Not connected\n\nStart the mirror on the device, then Connect.\nUSB defaults to 172.16.42.1."

        root.addSubview(topBar)
        root.addSubview(mirrorView)
        root.addSubview(bottomBar)

        NSLayoutConstraint.activate([
            topBar.topAnchor.constraint(equalTo: root.topAnchor),
            topBar.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            topBar.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            topBar.heightAnchor.constraint(equalToConstant: 54),

            bottomBar.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            bottomBar.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            bottomBar.bottomAnchor.constraint(equalTo: root.bottomAnchor),
            bottomBar.heightAnchor.constraint(equalToConstant: 30),

            mirrorView.topAnchor.constraint(equalTo: topBar.bottomAnchor),
            mirrorView.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            mirrorView.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            mirrorView.bottomAnchor.constraint(equalTo: bottomBar.topAnchor)
        ])

        configureTopBar(topBar)
        configureBottomBar(bottomBar)
        wireInput()
    }

    private func configureTopBar(_ bar: NSView) {
        hostField.translatesAutoresizingMaskIntoConstraints = false
        hostField.font = .monospacedDigitSystemFont(ofSize: 12, weight: .regular)
        hostField.placeholderString = "172.16.42.1"
        hostField.isBezeled = true
        hostField.bezelStyle = .roundedBezel
        hostField.focusRingType = .none
        hostField.delegate = self
        hostField.target = self
        hostField.action = #selector(hostCommitted)
        hostField.toolTip = "Device address over USB (172.16.42.1) or Wi-Fi"
        hostField.setAccessibilityLabel("Device address")

        connectButton.translatesAutoresizingMaskIntoConstraints = false
        connectButton.bezelStyle = .rounded
        connectButton.bezelColor = .controlAccentColor
        connectButton.target = self
        connectButton.action = #selector(connectPressed)
        connectButton.toolTip = "Connect to the device's wayvnc (⌘R)"
        connectButton.setAccessibilityLabel("Connect")

        startButton.translatesAutoresizingMaskIntoConstraints = false
        startButton.bezelStyle = .rounded
        startButton.image = NSImage(systemSymbolName: "play.fill", accessibilityDescription: "Start on device")
        startButton.imagePosition = .imageOnly
        startButton.target = self
        startButton.action = #selector(startPressed)
        startButton.toolTip = "Start wayvnc on the device over SSH (⇧⌘R)"
        startButton.setAccessibilityLabel("Start on device")

        fullscreenButton.translatesAutoresizingMaskIntoConstraints = false
        fullscreenButton.bezelStyle = .rounded
        fullscreenButton.image = NSImage(
            systemSymbolName: "arrow.up.left.and.arrow.down.right",
            accessibilityDescription: "Fullscreen"
        )
        fullscreenButton.imagePosition = .imageOnly
        fullscreenButton.target = self
        fullscreenButton.action = #selector(toggleFullscreen)
        fullscreenButton.toolTip = "Fullscreen (⌘F)"
        fullscreenButton.setAccessibilityLabel("Fullscreen")

        for view in [hostField, connectButton, startButton, fullscreenButton] {
            bar.addSubview(view)
        }

        NSLayoutConstraint.activate([
            hostField.leadingAnchor.constraint(equalTo: bar.leadingAnchor, constant: 92),
            hostField.centerYAnchor.constraint(equalTo: bar.centerYAnchor, constant: 3),
            hostField.widthAnchor.constraint(equalToConstant: 126),
            hostField.heightAnchor.constraint(equalToConstant: 24),

            connectButton.leadingAnchor.constraint(equalTo: hostField.trailingAnchor, constant: 6),
            connectButton.centerYAnchor.constraint(equalTo: hostField.centerYAnchor),
            connectButton.heightAnchor.constraint(equalToConstant: 24),
            connectButton.widthAnchor.constraint(greaterThanOrEqualToConstant: 84),

            startButton.leadingAnchor.constraint(equalTo: connectButton.trailingAnchor, constant: 6),
            startButton.centerYAnchor.constraint(equalTo: hostField.centerYAnchor),
            startButton.widthAnchor.constraint(equalToConstant: 32),
            startButton.heightAnchor.constraint(equalToConstant: 24),

            fullscreenButton.trailingAnchor.constraint(equalTo: bar.trailingAnchor, constant: -10),
            fullscreenButton.centerYAnchor.constraint(equalTo: hostField.centerYAnchor),
            fullscreenButton.widthAnchor.constraint(equalToConstant: 32),
            fullscreenButton.heightAnchor.constraint(equalToConstant: 24)
        ])
    }

    private func configureBottomBar(_ bar: NSView) {
        statusDot.translatesAutoresizingMaskIntoConstraints = false
        statusDot.wantsLayer = true
        statusDot.layer?.cornerRadius = 4
        statusDot.layer?.backgroundColor = NSColor.systemGray.cgColor

        statusLabel.translatesAutoresizingMaskIntoConstraints = false
        statusLabel.font = .systemFont(ofSize: 11)
        statusLabel.textColor = .secondaryLabelColor
        statusLabel.lineBreakMode = .byTruncatingTail
        statusLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        statsLabel.translatesAutoresizingMaskIntoConstraints = false
        statsLabel.font = .monospacedDigitSystemFont(ofSize: 11, weight: .regular)
        statsLabel.textColor = .tertiaryLabelColor
        statsLabel.alignment = .right

        for view in [statusDot, statusLabel, statsLabel] {
            bar.addSubview(view)
        }

        NSLayoutConstraint.activate([
            statusDot.leadingAnchor.constraint(equalTo: bar.leadingAnchor, constant: 12),
            statusDot.centerYAnchor.constraint(equalTo: bar.centerYAnchor),
            statusDot.widthAnchor.constraint(equalToConstant: 8),
            statusDot.heightAnchor.constraint(equalToConstant: 8),

            statusLabel.leadingAnchor.constraint(equalTo: statusDot.trailingAnchor, constant: 7),
            statusLabel.centerYAnchor.constraint(equalTo: bar.centerYAnchor),

            statsLabel.leadingAnchor.constraint(greaterThanOrEqualTo: statusLabel.trailingAnchor, constant: 8),
            statsLabel.trailingAnchor.constraint(equalTo: bar.trailingAnchor, constant: -12),
            statsLabel.centerYAnchor.constraint(equalTo: bar.centerYAnchor)
        ])
    }

    private func wireInput() {
        mirrorView.onPointer = { [weak self] x, y, buttons in
            self?.client?.sendPointer(x: x, y: y, buttons: buttons)
        }
        mirrorView.onKeyDown = { [weak self] event in
            guard let self, let client = self.client else { return }
            // Command-key shortcuts belong to the Mac app's menus.
            guard !event.modifierFlags.contains(.command) else { return }
            guard let keysym = KeyMap.keysym(for: event) else { return }
            client.sendKey(keysym: keysym, down: true)
            client.sendKey(keysym: keysym, down: false)
        }
        mirrorView.onKeyUp = { _ in }
        mirrorView.onFlagsChanged = { [weak self] event in
            guard let self, let client = self.client,
                  let modifier = KeyMap.modifier(for: event) else { return }
            client.sendKey(keysym: modifier.keysym, down: modifier.down)
        }
    }

    // MARK: - Lifecycle

    override func windowDidLoad() {
        super.windowDidLoad()
        window?.makeFirstResponder(mirrorView)
    }

    func windowWillClose(_ notification: Notification) {
        stopClient(announce: false)
        fpsTimer?.invalidate()
        fpsTimer = nil
    }

    private func startFPSClock() {
        let timer = Timer(timeInterval: 1, repeats: true) { [weak self] _ in
            self?.tick()
        }
        RunLoop.main.add(timer, forMode: .common)
        fpsTimer = timer
    }

    private func tick() {
        let fps = framesThisSecond
        framesThisSecond = 0
        if connected, currentSize.width > 0 {
            let rate = fps == 0 ? "idle" : "\(fps) fps"
            statsLabel.stringValue = "\(Int(currentSize.width))×\(Int(currentSize.height)) · \(rate)"
        } else {
            statsLabel.stringValue = "—"
        }
    }

    // MARK: - Actions

    @objc func connectPressed() {
        if client != nil {
            stopClient(announce: true)
        } else {
            connectToDevice()
        }
    }

    func connectToDevice() {
        userStopped = false
        retryAttempts = 0
        retryTimer?.invalidate()
        startConnection()
        scheduleRetries()
    }

    /// Starts one RFB attempt. Local Network permission can swallow the first few
    /// attempts while the system sheet is open, so failures are retried by the timer.
    private func startConnection() {
        guard !userStopped, client == nil else { return }
        let host = normalizedHost()
        guard !host.isEmpty else {
            NSSound.beep()
            retryTimer?.invalidate()
            return
        }
        stopClient(announce: false)
        setStatus("Connecting to \(host)…", color: .systemYellow)
        connectButton.title = "Stop"
        LocalNetworkPermission.trigger(host: host, port: 5900)

        let client = RFBClient(host: host)
        client.delegate = self
        self.client = client
        client.connect()
    }

    private func scheduleRetries() {
        retryTimer?.invalidate()
        let timer = Timer(timeInterval: 2.5, repeats: true) { [weak self] _ in
            guard let self else { return }
            if self.userStopped || self.connected {
                self.retryTimer?.invalidate()
                return
            }
            guard self.client == nil else { return }
            self.retryAttempts += 1
            guard self.retryAttempts <= 10 else {
                self.retryTimer?.invalidate()
                self.setStatus("Could not reach the device", color: .systemRed)
                return
            }
            self.startConnection()
        }
        RunLoop.main.add(timer, forMode: .common)
        retryTimer = timer
    }

    @objc func startPressed() {
        let host = normalizedHost()
        guard !host.isEmpty else {
            NSSound.beep()
            return
        }
        startButton.isEnabled = false
        setStatus("Starting wayvnc on the device…", color: .systemYellow)
        DeviceMirrorService.start(host: host) { [weak self] failure in
            guard let self else { return }
            self.startButton.isEnabled = true
            if let failure {
                self.setStatus(failure, color: .systemRed)
            } else if self.connected {
                // wayvnc was already up; keep the existing session instead of
                // tearing it down just because Start was pressed again.
                self.setStatus("Connected", color: .systemGreen)
            } else {
                self.setStatus("wayvnc is listening", color: .systemGreen)
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) {
                    self.connectToDevice()
                }
            }
        }
    }

    @objc func toggleFullscreen() {
        window?.toggleFullScreen(nil)
    }

    @objc private func hostCommitted() {
        if client == nil {
            connectToDevice()
        }
        window?.makeFirstResponder(mirrorView)
    }

    private func normalizedHost() -> String {
        var host = hostField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        if host.hasPrefix("vnc://") {
            host.removeFirst("vnc://".count)
        }
        // Accept host:port (a single colon); RFBClient always uses 5900.
        if host.filter({ $0 == ":" }).count == 1 {
            host = String(host.split(separator: ":", maxSplits: 1).first ?? "")
        }
        return host
    }

    private func stopClient(announce: Bool) {
        if let client {
            client.delegate = nil
            client.disconnect()
        }
        client = nil
        connected = false
        connectButton.title = "Connect"
        mirrorView.image = nil
        mirrorView.placeholderText = "Not connected\n\nStart the mirror on the device, then Connect.\nUSB defaults to 172.16.42.1."
        if announce {
            userStopped = true
            retryTimer?.invalidate()
            setStatus("Disconnected", color: .systemGray)
        }
    }

    private func setStatus(_ text: String, color: NSColor) {
        statusLabel.stringValue = text
        statusDot.layer?.backgroundColor = color.cgColor
    }

    // MARK: - RFBClientDelegate

    func rfbClient(_ client: RFBClient, didConnectWith size: CGSize) {
        guard client === self.client else { return }
        connected = true
        retryAttempts = 0
        retryTimer?.invalidate()
        currentSize = size
        mirrorView.framebufferSize = size
        connectButton.title = "Stop"
        setStatus("Connected", color: .systemGreen)
        window?.makeFirstResponder(mirrorView)
        tick()
    }

    func rfbClient(_ client: RFBClient, didUpdate image: CGImage) {
        guard client === self.client else { return }
        framesThisSecond += 1
        let size = CGSize(width: image.width, height: image.height)
        if size != currentSize {
            currentSize = size
            mirrorView.framebufferSize = size
        }
        mirrorView.image = image
    }

    func rfbClient(_ client: RFBClient, didDisconnect reason: String) {
        guard client === self.client else { return }
        self.client = nil
        connected = false
        connectButton.title = "Connect"
        setStatus(reason, color: .systemRed)
        mirrorView.image = nil
        mirrorView.placeholderText = "\(reason)\n\nPress Connect to retry, or Start to relaunch wayvnc."
        tick()
    }
}
