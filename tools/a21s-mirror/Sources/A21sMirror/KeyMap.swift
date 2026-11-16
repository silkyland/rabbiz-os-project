import AppKit

/// Translation from AppKit key events to X11 keysyms (what RFB KeyEvent carries).
enum KeyMap {
    /// Non-printing keys. Everything else falls through to the character the key
    /// produced (ignoring Shift, which is sent separately as a modifier).
    private static let specialKeys: [UInt16: UInt32] = [
        36: 0xFF0D,  // Return
        48: 0xFF09,  // Tab
        51: 0xFF08,  // Backspace
        53: 0xFF1B,  // Escape
        117: 0xFFFF, // Forward delete
        115: 0xFF50, // Home
        119: 0xFF57, // End
        116: 0xFF55, // Page up
        121: 0xFF56, // Page down
        123: 0xFF51, // Left
        124: 0xFF53, // Right
        125: 0xFF54, // Down
        126: 0xFF52, // Up
        122: 0xFFBE, 120: 0xFFBF, 99: 0xFFC0, 118: 0xFFC1,
        96: 0xFFC2, 97: 0xFFC3, 98: 0xFFC4, 100: 0xFFC5,
        101: 0xFFC6, 109: 0xFFC7, 103: 0xFFC8, 111: 0xFFC9,
        105: 0xFFCA
    ]

    static func keysym(for event: NSEvent) -> UInt32? {
        if let special = specialKeys[event.keyCode] {
            return special
        }
        guard let characters = event.charactersIgnoringModifiers,
              let scalar = characters.unicodeScalars.first else {
            return nil
        }
        let value = scalar.value
        if value < 0x80 {
            return value
        }
        // Unicode keysym (X11 convention): 0x01000000 + code point.
        return 0x0100_0000 | value
    }

    /// Modifier keys are sent on `flagsChanged`, so their state is shared with
    /// character keys (Shift+1, Ctrl+C, Cmd as Super).
    static func modifier(for event: NSEvent) -> (keysym: UInt32, down: Bool)? {
        let flags = event.modifierFlags
        switch event.keyCode {
        case 56: return (0xFFE1, flags.contains(.shift))
        case 60: return (0xFFE2, flags.contains(.shift))
        case 59: return (0xFFE3, flags.contains(.control))
        case 62: return (0xFFE4, flags.contains(.control))
        case 58: return (0xFFE9, flags.contains(.option))
        case 61: return (0xFFEA, flags.contains(.option))
        case 55: return (0xFFEB, flags.contains(.command))
        case 54: return (0xFFEC, flags.contains(.command))
        case 57: return (0xFFE5, flags.contains(.capsLock))
        default: return nil
        }
    }
}
