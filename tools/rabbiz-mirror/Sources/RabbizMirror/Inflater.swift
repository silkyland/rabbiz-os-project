import Foundation
import zlib

/// One persistent zlib stream for one RFB connection.
///
/// ZRLE keeps a single inflate stream alive for the whole session, flushing it at
/// the end of every rectangle. That means state must survive between calls; a
/// fresh `inflateInit` per rectangle would fail on the second message.
final class Inflater {
    private var stream = z_stream()
    private var initialised = false
    private var output = [UInt8](repeating: 0, count: 1 << 20)

    init?() {
        guard inflateInit_(&stream, ZLIB_VERSION, Int32(MemoryLayout<z_stream>.size)) == Z_OK else {
            return nil
        }
        initialised = true
    }

    deinit {
        if initialised {
            inflateEnd(&stream)
        }
    }

    /// Inflates one ZRLE block. `expected` is the uncompressed upper bound for the
    /// rectangle (3 bytes per pixel plus a byte per tile), used to size the buffer.
    func inflate(_ input: [UInt8], expected: Int) throws -> [UInt8] {
        let capacity = max(expected + 65_536, 1 << 16)
        if output.count < capacity {
            output = [UInt8](repeating: 0, count: capacity)
        }

        var produced = 0
        var status: Int32 = Z_OK

        try input.withUnsafeBufferPointer { inputBuffer in
            guard let inputBase = inputBuffer.baseAddress else {
                throw RFBError.protocolViolation("empty zlib block")
            }
            stream.next_in = UnsafeMutablePointer(mutating: inputBase)
            stream.avail_in = uInt(inputBuffer.count)

            while true {
                let space = output.count - produced
                guard space > 0 else {
                    throw RFBError.protocolViolation("zlib block larger than expected")
                }
                status = output.withUnsafeMutableBufferPointer { outBuffer -> Int32 in
                    stream.next_out = outBuffer.baseAddress! + produced
                    stream.avail_out = uInt(space)
                    return zlib.inflate(&stream, Z_SYNC_FLUSH)
                }
                produced += space - Int(stream.avail_out)

                if stream.avail_in == 0 && status != Z_BUF_ERROR {
                    break
                }
                if status != Z_OK && status != Z_BUF_ERROR {
                    break
                }
                if stream.avail_in == 0 {
                    // Z_SYNC_FLUSH can leave a few pending bytes with no input left.
                    let before = produced
                    status = output.withUnsafeMutableBufferPointer { outBuffer -> Int32 in
                        stream.next_out = outBuffer.baseAddress! + produced
                        stream.avail_out = uInt(output.count - produced)
                        stream.next_in = nil
                        stream.avail_in = 0
                        return zlib.inflate(&stream, Z_SYNC_FLUSH)
                    }
                    produced += (output.count - produced) - Int(stream.avail_out)
                    if produced == before || status == Z_STREAM_END {
                        break
                    }
                }
            }
        }

        guard status == Z_OK || status == Z_STREAM_END || status == Z_BUF_ERROR else {
            throw RFBError.protocolViolation("zlib inflate failed (\(status))")
        }
        guard stream.avail_in == 0 else {
            throw RFBError.protocolViolation("zlib block truncated")
        }
        return Array(output[0..<produced])
    }
}
