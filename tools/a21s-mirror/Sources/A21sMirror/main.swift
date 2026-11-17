import AppKit

if CommandLine.arguments.contains("--selftest") {
    MirrorSelfTest.run(arguments: CommandLine.arguments)
}

let application = NSApplication.shared
let appDelegate = AppDelegate()
application.delegate = appDelegate
application.run()
