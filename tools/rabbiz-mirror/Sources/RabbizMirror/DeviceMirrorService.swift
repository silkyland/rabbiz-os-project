import AppKit

/// Starts the device-side wayvnc unit through SSH, so the app is self-contained:
/// plug in USB, hit Start, hit Connect.
enum DeviceMirrorService {
    static func start(host: String, completion: @escaping (String?) -> Void) {
        let trimmed = host.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            completion("Enter the device address first.")
            return
        }
        guard isValidHost(trimmed) else {
            completion("Invalid device address.")
            return
        }
        let target = trimmed.contains("@") ? trimmed : "root@\(trimmed)"
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/ssh")
        process.arguments = [
            "-o", "BatchMode=yes",
            "-o", "ConnectTimeout=6",
            "-o", "StrictHostKeyChecking=accept-new",
            "--",
            target,
            "rabbiz-mirror", "start"
        ]
        let stdout = Pipe()
        let stderr = Pipe()
        process.standardOutput = stdout
        process.standardError = stderr
        process.terminationHandler = { finished in
            _ = stdout.fileHandleForReading.readDataToEndOfFile()
            let error = String(data: stderr.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
            DispatchQueue.main.async {
                if finished.terminationStatus == 0 {
                    completion(nil)
                } else {
                    let message = error.isEmpty
                        ? "ssh exited with status \(finished.terminationStatus)"
                        : firstLines(error)
                    completion(message)
                }
            }
        }
        do {
            try process.run()
        } catch {
            DispatchQueue.main.async {
                completion("Could not run ssh: \(error.localizedDescription)")
            }
        }
    }

    private static func isValidHost(_ host: String) -> Bool {
        guard !host.isEmpty, host.count <= 255, !host.hasPrefix("-") else {
            return false
        }
        let allowed = CharacterSet(charactersIn: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789.-_:@[]")
        guard host.unicodeScalars.allSatisfy({ allowed.contains($0) }) else {
            return false
        }
        let parts = host.split(separator: "@", omittingEmptySubsequences: false)
        if parts.count > 2 { return false }
        if parts.count == 2 {
            let user = parts[0]
            let hostname = parts[1]
            guard !user.isEmpty, !hostname.isEmpty,
                  !user.hasPrefix("-"), !hostname.hasPrefix("-") else {
                return false
            }
        }
        return true
    }

    private static func firstLines(_ text: String) -> String {
        let lines = text
            .split(separator: "\n", omittingEmptySubsequences: true)
            .prefix(2)
            .map(String.init)
        return lines.joined(separator: " ")
    }
}
