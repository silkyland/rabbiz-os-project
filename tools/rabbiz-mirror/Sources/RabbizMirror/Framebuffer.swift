import CoreGraphics
import Foundation

/// The light of the phone's output, as one BGRA bitmap.
///
/// Pixel row 0 of `CGContext`'s backing store is the *top* row of the image, which
/// is exactly how RFB numbers its rows, so VNC pixels can be written straight in
/// without a flip.
final class Framebuffer {
    private(set) var width = 0
    private(set) var height = 0
    private var context: CGContext?
    private let lock = NSLock()

    func resize(width newWidth: Int, height newHeight: Int) {
        guard newWidth > 0, newHeight > 0 else { return }
        lock.lock()
        defer { lock.unlock() }
        guard newWidth != width || newHeight != height else { return }

        width = newWidth
        height = newHeight
        let space = CGColorSpaceCreateDeviceRGB()
        let info = CGImageAlphaInfo.noneSkipFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue
        context = CGContext(
            data: nil,
            width: newWidth,
            height: newHeight,
            bitsPerComponent: 8,
            bytesPerRow: newWidth * 4,
            space: space,
            bitmapInfo: info
        )
        context?.setFillColor(CGColor(red: 0, green: 0, blue: 0, alpha: 1))
        context?.fill(CGRect(x: 0, y: 0, width: newWidth, height: newHeight))
    }

    /// Runs `body` with the raw BGRA bytes while the bitmap is locked.
    func withPixels<T>(_ body: (UnsafeMutablePointer<UInt8>, Int) throws -> T) rethrows -> T? {
        lock.lock()
        defer { lock.unlock() }
        guard let context, let data = context.data else { return nil }
        return try body(data.assumingMemoryBound(to: UInt8.self), context.bytesPerRow)
    }

    func currentSize() -> (width: Int, height: Int) {
        lock.lock()
        defer { lock.unlock() }
        return (width, height)
    }

    func snapshot() -> CGImage? {
        lock.lock()
        defer { lock.unlock() }
        return context?.makeImage()
    }
}
