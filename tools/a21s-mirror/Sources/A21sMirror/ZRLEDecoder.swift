import Foundation

/// ZRLE tile decoding (RFC 6143 §7.7.6, tiles encoded as TRLE §7.7.5).
///
/// With the pixel format we negotiate (32 bpp, depth 24, little endian, shifts
/// 16/8/0) a CPIXEL is 3 bytes stored B, G, R — the low three bytes of the
/// little-endian pixel. The framebuffer is BGRA, so those map straight in.
enum ZRLEDecoder {
    static func decode(
        _ data: [UInt8],
        x: Int,
        y: Int,
        width: Int,
        height: Int,
        frame: UnsafeMutablePointer<UInt8>,
        frameWidth: Int
    ) throws {
        guard width > 0, height > 0 else { return }

        try data.withUnsafeBufferPointer { buffer in
            guard let base = buffer.baseAddress else {
                throw RFBError.protocolViolation("empty ZRLE block")
            }
            let end = base + buffer.count
            var cursor = base
            var palette = [UInt8](repeating: 0, count: 16 * 3)

            func nextByte() throws -> UInt8 {
                guard cursor < end else {
                    throw RFBError.protocolViolation("ZRLE overrun")
                }
                defer { cursor += 1 }
                return cursor.pointee
            }

            func nextCPixel() throws -> (UInt8, UInt8, UInt8) {
                guard cursor + 3 <= end else {
                    throw RFBError.protocolViolation("ZRLE overrun in colour")
                }
                let blue = cursor[0]
                let green = cursor[1]
                let red = cursor[2]
                cursor += 3
                return (red, green, blue)
            }

            func nextRunLength() throws -> Int {
                var length = 1
                while true {
                    let byte = try nextByte()
                    length += Int(byte)
                    if byte != 255 { return length }
                }
            }

            func write(_ red: UInt8, _ green: UInt8, _ blue: UInt8, at offset: Int) {
                frame[offset] = blue
                frame[offset + 1] = green
                frame[offset + 2] = red
                frame[offset + 3] = 255
            }

            let tilesX = (width + 63) / 64
            let tilesY = (height + 63) / 64

            for tileY in 0..<tilesY {
                for tileX in 0..<tilesX {
                    let tileWidth = min(64, width - tileX * 64)
                    let tileHeight = min(64, height - tileY * 64)
                    let pixelCount = tileWidth * tileHeight
                    let originX = x + tileX * 64
                    let originY = y + tileY * 64
                    let subencoding = try nextByte()

                    switch subencoding {
                    case 0: // raw CPIXELs
                        for row in 0..<tileHeight {
                            var offset = ((originY + row) * frameWidth + originX) * 4
                            for _ in 0..<tileWidth {
                                let colour = try nextCPixel()
                                write(colour.0, colour.1, colour.2, at: offset)
                                offset += 4
                            }
                        }

                    case 1: // solid
                        let colour = try nextCPixel()
                        for row in 0..<tileHeight {
                            var offset = ((originY + row) * frameWidth + originX) * 4
                            for _ in 0..<tileWidth {
                                write(colour.0, colour.1, colour.2, at: offset)
                                offset += 4
                            }
                        }

                    case 2...16: // packed palette
                        let paletteSize = Int(subencoding)
                        for index in 0..<paletteSize {
                            let colour = try nextCPixel()
                            palette[index * 3] = colour.0
                            palette[index * 3 + 1] = colour.1
                            palette[index * 3 + 2] = colour.2
                        }
                        let bits = paletteSize == 2 ? 1 : (paletteSize <= 4 ? 2 : 4)
                        let rowBytes = (tileWidth * bits + 7) / 8
                        let mask = UInt8((1 << bits) - 1)

                        for row in 0..<tileHeight {
                            guard cursor + rowBytes <= end else {
                                throw RFBError.protocolViolation("ZRLE packed palette overrun")
                            }
                            var offset = ((originY + row) * frameWidth + originX) * 4
                            var bitIndex = 0
                            for _ in 0..<tileWidth {
                                let byte = cursor[bitIndex >> 3]
                                let shift = 8 - bits - (bitIndex & 7)
                                let index = Int((byte >> UInt8(shift)) & mask)
                                write(
                                    palette[index * 3],
                                    palette[index * 3 + 1],
                                    palette[index * 3 + 2],
                                    at: offset
                                )
                                offset += 4
                                bitIndex += bits
                            }
                            cursor += rowBytes
                        }

                    case 128: // plain RLE
                        var done = 0
                        while done < pixelCount {
                            let colour = try nextCPixel()
                            let run = try nextRunLength()
                            let count = min(run, pixelCount - done)
                            for step in 0..<count {
                                let pixel = done + step
                                let offset = ((originY + pixel / tileWidth) * frameWidth
                                              + originX + pixel % tileWidth) * 4
                                write(colour.0, colour.1, colour.2, at: offset)
                            }
                            done += count
                        }

                    case 130...255: // palette RLE
                        let paletteSize = Int(subencoding) - 128
                        for index in 0..<paletteSize {
                            let colour = try nextCPixel()
                            palette[index * 3] = colour.0
                            palette[index * 3 + 1] = colour.1
                            palette[index * 3 + 2] = colour.2
                        }
                        var done = 0
                        while done < pixelCount {
                            let byte = try nextByte()
                            let index: Int
                            let run: Int
                            if byte & 0x80 != 0 {
                                index = Int(byte & 0x7F)
                                run = try nextRunLength()
                            } else {
                                index = Int(byte)
                                run = 1
                            }
                            guard index < paletteSize else {
                                throw RFBError.protocolViolation("ZRLE palette index out of range")
                            }
                            let count = min(run, pixelCount - done)
                            for step in 0..<count {
                                let pixel = done + step
                                let offset = ((originY + pixel / tileWidth) * frameWidth
                                              + originX + pixel % tileWidth) * 4
                                write(
                                    palette[index * 3],
                                    palette[index * 3 + 1],
                                    palette[index * 3 + 2],
                                    at: offset
                                )
                            }
                            done += count
                        }

                    default:
                        throw RFBError.protocolViolation("unsupported ZRLE subencoding \(subencoding)")
                    }
                }
            }
        }
    }
}
