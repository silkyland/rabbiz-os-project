import AppKit

/// Draws the mirrored framebuffer aspect-fit, and translates AppKit mouse/key
/// events into RFB pointer/key events.
final class MirrorView: NSView {
    var image: CGImage? {
        didSet { needsDisplay = true }
    }
    var framebufferSize = CGSize(width: 720, height: 1600)
    var placeholderText = "Not connected"

    var onPointer: ((Int, Int, Int) -> Void)?
    var onKeyDown: ((NSEvent) -> Void)?
    var onKeyUp: ((NSEvent) -> Void)?
    var onFlagsChanged: ((NSEvent) -> Void)?

    private var buttons = 0
    private var trackingArea: NSTrackingArea?
    private var preciseScroll: CGFloat = 0
    private var lastPoint = CGPoint.zero

    override var acceptsFirstResponder: Bool { true }
    override var isOpaque: Bool { true }

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        layer?.backgroundColor = NSColor.black.cgColor
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) is not used")
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let trackingArea {
            removeTrackingArea(trackingArea)
        }
        let area = NSTrackingArea(
            rect: bounds,
            options: [.mouseMoved, .activeInKeyWindow, .inVisibleRect],
            owner: self,
            userInfo: nil
        )
        addTrackingArea(area)
        trackingArea = area
    }

    override func resetCursorRects() {
        addCursorRect(bounds, cursor: .arrow)
    }

    // MARK: - Drawing

    private var imageRect: NSRect {
        guard let image, image.width > 0, image.height > 0,
              bounds.width > 1, bounds.height > 1 else { return .zero }
        let aspect = CGFloat(image.width) / CGFloat(image.height)
        var size = bounds.size
        if bounds.width / bounds.height > aspect {
            size.width = bounds.height * aspect
        } else {
            size.height = bounds.width / aspect
        }
        return NSRect(
            x: (bounds.width - size.width) / 2,
            y: (bounds.height - size.height) / 2,
            width: size.width,
            height: size.height
        )
    }

    override func draw(_ dirtyRect: NSRect) {
        NSColor.black.setFill()
        bounds.fill()

        guard let context = NSGraphicsContext.current?.cgContext else { return }
        if let image {
            context.saveGState()
            context.interpolationQuality = .high
            context.draw(image, in: imageRect)
            context.restoreGState()
        } else {
            drawPlaceholder()
        }
    }

    private func drawPlaceholder() {
        let paragraph = NSMutableParagraphStyle()
        paragraph.alignment = .center
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 13, weight: .medium),
            .foregroundColor: NSColor.tertiaryLabelColor,
            .paragraphStyle: paragraph
        ]
        let text = NSAttributedString(string: placeholderText, attributes: attributes)
        let height = text.boundingRect(
            with: NSSize(width: bounds.width - 48, height: 200),
            options: [.usesLineFragmentOrigin]
        ).height
        text.draw(
            in: NSRect(
                x: 24,
                y: bounds.midY - height / 2,
                width: bounds.width - 48,
                height: height
            )
        )
    }

    // MARK: - Pointer

    private func sendPointer(_ event: NSEvent, mask: Int) {
        let point = convert(event.locationInWindow, from: nil)
        lastPoint = point
        let rect = imageRect
        guard rect.width > 0, rect.height > 0 else { return }
        let normalX = min(max((point.x - rect.minX) / rect.width, 0), 1)
        let normalY = min(max((rect.maxY - point.y) / rect.height, 0), 1)
        let x = Int((normalX * CGFloat(framebufferSize.width)).rounded())
        let y = Int((normalY * CGFloat(framebufferSize.height)).rounded())
        if ProcessInfo.processInfo.environment["A21S_MIRROR_DEBUG"] != nil {
            print("pointer: window=\(self.window?.frame ?? .zero) event=\(event.locationInWindow) view=\(point) bounds=\(bounds) imageRect=\(rect) fb=\(framebufferSize) -> \(x),\(y)")
        }
        onPointer?(x, y, mask)
    }

    private func sendWheel(_ up: Bool) {
        let rect = imageRect
        guard rect.width > 0, rect.height > 0 else { return }
        let normalX = min(max((lastPoint.x - rect.minX) / rect.width, 0), 1)
        let normalY = min(max((rect.maxY - lastPoint.y) / rect.height, 0), 1)
        let x = Int((normalX * CGFloat(framebufferSize.width)).rounded())
        let y = Int((normalY * CGFloat(framebufferSize.height)).rounded())
        let wheelButton = up ? 8 : 16
        onPointer?(x, y, buttons | wheelButton)
        onPointer?(x, y, buttons)
    }

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        buttons |= 1
        sendPointer(event, mask: buttons)
    }

    override func mouseUp(with event: NSEvent) {
        buttons &= ~1
        sendPointer(event, mask: buttons)
    }

    override func rightMouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        buttons |= 4
        sendPointer(event, mask: buttons)
    }

    override func rightMouseUp(with event: NSEvent) {
        buttons &= ~4
        sendPointer(event, mask: buttons)
    }

    override func otherMouseDown(with event: NSEvent) {
        buttons |= 2
        sendPointer(event, mask: buttons)
    }

    override func otherMouseUp(with event: NSEvent) {
        buttons &= ~2
        sendPointer(event, mask: buttons)
    }

    override func mouseMoved(with event: NSEvent) {
        sendPointer(event, mask: buttons)
    }

    override func mouseDragged(with event: NSEvent) {
        sendPointer(event, mask: buttons)
    }

    override func rightMouseDragged(with event: NSEvent) {
        sendPointer(event, mask: buttons)
    }

    override func otherMouseDragged(with event: NSEvent) {
        sendPointer(event, mask: buttons)
    }

    override func scrollWheel(with event: NSEvent) {
        lastPoint = convert(event.locationInWindow, from: nil)
        let deltaY = event.scrollingDeltaY
        let deltaX = event.scrollingDeltaX
        guard abs(deltaY) + abs(deltaX) > 0.01 else { return }

        if abs(deltaX) > abs(deltaY) {
            sendHorizontalWheel(right: deltaX > 0)
            return
        }
        if event.hasPreciseScrollingDeltas {
            preciseScroll += deltaY
            while abs(preciseScroll) >= 12 {
                let up = preciseScroll > 0
                sendWheel(up)
                preciseScroll -= up ? 12 : -12
            }
        } else {
            sendWheel(deltaY > 0)
        }
    }

    private func sendHorizontalWheel(right: Bool) {
        let rect = imageRect
        guard rect.width > 0, rect.height > 0 else { return }
        let normalX = min(max((lastPoint.x - rect.minX) / rect.width, 0), 1)
        let normalY = min(max((rect.maxY - lastPoint.y) / rect.height, 0), 1)
        let x = Int((normalX * CGFloat(framebufferSize.width)).rounded())
        let y = Int((normalY * CGFloat(framebufferSize.height)).rounded())
        let button = right ? 64 : 32
        onPointer?(x, y, buttons | button)
        onPointer?(x, y, buttons)
    }

    // MARK: - Keyboard

    override func keyDown(with event: NSEvent) {
        onKeyDown?(event)
    }

    override func keyUp(with event: NSEvent) {
        onKeyUp?(event)
    }

    override func flagsChanged(with event: NSEvent) {
        onFlagsChanged?(event)
    }
}
