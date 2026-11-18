import AppKit

final class AppDelegate: NSObject, NSApplicationDelegate {
    private var windowController: MainWindowController?
    private var snapshotPath: String?

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        NSApp.appearance = NSAppearance(named: .darkAqua)
        buildMenu()

        let controller = MainWindowController()
        windowController = controller
        controller.showWindow(nil)
        controller.window?.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)

        // Try the device straight away: if wayvnc is already running this is instant,
        // and if not the status line explains what to press.
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.6) {
            controller.connectToDevice()
        }

        if let index = CommandLine.arguments.firstIndex(of: "--snapshot"),
           index + 1 < CommandLine.arguments.count {
            snapshotPath = CommandLine.arguments[index + 1]
            DispatchQueue.main.asyncAfter(deadline: .now() + 2.5) {
                self.captureSnapshot()
            }
        }
    }

    /// Renders the app's own window to PNG (no screen-recording permission needed).
    private func captureSnapshot() {
        defer { NSApp.terminate(nil) }
        guard let path = snapshotPath, let content = windowController?.window?.contentView,
              let representation = content.bitmapImageRepForCachingDisplay(in: content.bounds)
        else { return }
        content.cacheDisplay(in: content.bounds, to: representation)
        guard let data = representation.representation(using: .png, properties: [:]) else { return }
        do {
            try data.write(to: URL(fileURLWithPath: path))
            print("snapshot: wrote \(path)")
        } catch {
            print("snapshot: failed — \(error.localizedDescription)")
        }
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }

    func applicationSupportsSecureRestorableState(_ app: NSApplication) -> Bool {
        true
    }

    // MARK: - Menu

    private func buildMenu() {
        let mainMenu = NSMenu()

        let appItem = NSMenuItem()
        mainMenu.addItem(appItem)
        let appMenu = NSMenu()
        appMenu.addItem(
            withTitle: "About Rabbiz Mirror",
            action: #selector(NSApplication.orderFrontStandardAboutPanel(_:)),
            keyEquivalent: ""
        )
        appMenu.addItem(.separator())
        appMenu.addItem(
            withTitle: "Hide Rabbiz Mirror",
            action: #selector(NSApplication.hide(_:)),
            keyEquivalent: "h"
        )
        let hideOthers = appMenu.addItem(
            withTitle: "Hide Others",
            action: #selector(NSApplication.hideOtherApplications(_:)),
            keyEquivalent: "h"
        )
        hideOthers.keyEquivalentModifierMask = [.command, .option]
        appMenu.addItem(
            withTitle: "Show All",
            action: #selector(NSApplication.unhideAllApplications(_:)),
            keyEquivalent: ""
        )
        appMenu.addItem(.separator())
        appMenu.addItem(
            withTitle: "Quit Rabbiz Mirror",
            action: #selector(NSApplication.terminate(_:)),
            keyEquivalent: "q"
        )
        appItem.submenu = appMenu

        let editItem = NSMenuItem()
        mainMenu.addItem(editItem)
        let editMenu = NSMenu(title: "Edit")
        editMenu.addItem(withTitle: "Undo", action: Selector(("undo:")), keyEquivalent: "z")
        editMenu.addItem(withTitle: "Redo", action: Selector(("redo:")), keyEquivalent: "Z")
        editMenu.addItem(.separator())
        editMenu.addItem(withTitle: "Cut", action: #selector(NSText.cut(_:)), keyEquivalent: "x")
        editMenu.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        editMenu.addItem(withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
        editMenu.addItem(withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
        editItem.submenu = editMenu

        let connectionItem = NSMenuItem()
        mainMenu.addItem(connectionItem)
        let connectionMenu = NSMenu(title: "Connection")
        let connect = connectionMenu.addItem(
            withTitle: "Connect",
            action: #selector(connectMenu),
            keyEquivalent: "r"
        )
        connect.target = self
        let start = connectionMenu.addItem(
            withTitle: "Start on Device",
            action: #selector(startMenu),
            keyEquivalent: "R"
        )
        start.target = self
        start.keyEquivalentModifierMask = [.command, .shift]
        let disconnect = connectionMenu.addItem(
            withTitle: "Disconnect",
            action: #selector(disconnectMenu),
            keyEquivalent: "."
        )
        disconnect.target = self
        connectionMenu.addItem(.separator())
        let fullscreen = connectionMenu.addItem(
            withTitle: "Fullscreen",
            action: #selector(fullscreenMenu),
            keyEquivalent: "f"
        )
        fullscreen.target = self
        connectionItem.submenu = connectionMenu

        let windowItem = NSMenuItem()
        mainMenu.addItem(windowItem)
        let windowMenu = NSMenu(title: "Window")
        windowMenu.addItem(
            withTitle: "Minimize",
            action: #selector(NSWindow.performMiniaturize(_:)),
            keyEquivalent: "m"
        )
        windowMenu.addItem(
            withTitle: "Zoom",
            action: #selector(NSWindow.performZoom(_:)),
            keyEquivalent: ""
        )
        windowMenu.addItem(.separator())
        windowMenu.addItem(
            withTitle: "Close",
            action: #selector(NSWindow.performClose(_:)),
            keyEquivalent: "w"
        )
        windowItem.submenu = windowMenu

        NSApp.mainMenu = mainMenu
        NSApp.windowsMenu = windowMenu
    }

    @objc private func connectMenu() {
        windowController?.connectToDevice()
    }

    @objc private func startMenu() {
        windowController?.startPressed()
    }

    @objc private func disconnectMenu() {
        windowController?.connectPressed()
    }

    @objc private func fullscreenMenu() {
        windowController?.toggleFullscreen()
    }
}
