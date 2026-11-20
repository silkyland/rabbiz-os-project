/// Dart FFI bindings to `librabbiz_sdk_c` (`include/rabbiz_sdk.h`) with a
/// pure-Dart fallback so `flutter test` works on any desktop host without requiring
/// the native C shared library to be preloaded.
library rabbiz_sdk;

import 'dart:convert';
import 'dart:ffi' as ffi;
import 'dart:io' show Directory, File, Platform;

/// RGBA8 design token color matching `RabbizColor` in `rabbiz_sdk.h`.
class RabbizColor {
  final int r;
  final int g;
  final int b;
  final int a;

  const RabbizColor(this.r, this.g, this.b, [this.a = 0xFF]);

  int toArgb32() => ((a & 0xFF) << 24) | ((r & 0xFF) << 16) | ((g & 0xFF) << 8) | (b & 0xFF);
}

/// Active RabbizOS theme tokens ("Cream Glass" Light default + "Plum" Dark).
class RabbizTheme {
  final RabbizColor bg;
  final RabbizColor canvas;
  final RabbizColor surface1;
  final RabbizColor surface2;
  final RabbizColor surface3;
  final RabbizColor text;
  final RabbizColor text2;
  final RabbizColor text3;
  final RabbizColor textOnAccent;
  final RabbizColor accent;
  final RabbizColor danger;
  final RabbizColor success;
  final RabbizColor warning;
  final RabbizColor line;

  const RabbizTheme({
    required this.bg,
    required this.canvas,
    required this.surface1,
    required this.surface2,
    required this.surface3,
    required this.text,
    required this.text2,
    required this.text3,
    required this.textOnAccent,
    required this.accent,
    required this.danger,
    required this.success,
    required this.warning,
    required this.line,
  });

  /// Default v1 Light "Cream Glass" theme palette.
  static const RabbizTheme creamGlassLight = RabbizTheme(
    bg: RabbizColor(0xF4, 0xED, 0xE1),
    canvas: RabbizColor(0xF9, 0xF5, 0xEE),
    surface1: RabbizColor(0xFF, 0xF9, 0xF0),
    surface2: RabbizColor(0xFD, 0xFB, 0xF7),
    surface3: RabbizColor(0xEA, 0xDD, 0xC9),
    text: RabbizColor(0x30, 0x21, 0x35),
    text2: RabbizColor(0x58, 0x43, 0x5E),
    text3: RabbizColor(0x7A, 0x64, 0x82),
    textOnAccent: RabbizColor(0xFD, 0xFB, 0xF7),
    accent: RabbizColor(0xA6, 0x5A, 0x2E),
    danger: RabbizColor(0xD9, 0x5C, 0x51),
    success: RabbizColor(0x3F, 0x9E, 0x8A),
    warning: RabbizColor(0xA6, 0x7B, 0x28),
    line: RabbizColor(0x2A, 0x1D, 0x2F, 0x1F),
  );

  /// Dark "Plum" theme palette.
  static const RabbizTheme plumDark = RabbizTheme(
    bg: RabbizColor(0x16, 0x10, 0x1A),
    canvas: RabbizColor(0x1D, 0x15, 0x22),
    surface1: RabbizColor(0x24, 0x1A, 0x2A),
    surface2: RabbizColor(0x2E, 0x21, 0x36),
    surface3: RabbizColor(0x3A, 0x2A, 0x43),
    text: RabbizColor(0xF4, 0xED, 0xE1),
    text2: RabbizColor(0xCD, 0xBF, 0xC4),
    text3: RabbizColor(0x9C, 0x8A, 0x93),
    textOnAccent: RabbizColor(0x2A, 0x17, 0x10),
    accent: RabbizColor(0xD0, 0x8A, 0x5A),
    danger: RabbizColor(0xE2, 0x80, 0x7F),
    success: RabbizColor(0x86, 0xC9, 0xA3),
    warning: RabbizColor(0xE8, 0xC4, 0x6A),
    line: RabbizColor(0xF4, 0xED, 0xE1, 0x1A),
  );
}

/// Safe-area window insets on RabbizOS devices (`720x1600` A21s / `1600x2560` Muyu).
class RabbizInsets {
  final int top;
  final int right;
  final int bottom;
  final int left;
  final int imeBottom;

  const RabbizInsets({
    this.top = 56,
    this.right = 0,
    this.bottom = 32,
    this.left = 0,
    this.imeBottom = 0,
  });
}

/// Cross-app URI intent helper (`tel:`, `sms:`, `file://`, `rabbiz://`).
class RabbizIntent {
  final String uri;
  const RabbizIntent(this.uri);

  static bool isValidUri(String raw) {
    final trimmed = raw.trim();
    return trimmed.startsWith('tel:') ||
        trimmed.startsWith('sms:') ||
        trimmed.startsWith('file://') ||
        trimmed.startsWith('rabbiz://');
  }
}

/// App-scoped XDG storage helper with atomic JSON file writes.
class RabbizStorage {
  final String appId;
  const RabbizStorage(this.appId);

  Directory dataDir() {
    final xdg = Platform.environment['XDG_DATA_HOME'];
    final base = (xdg != null && xdg.isNotEmpty)
        ? xdg
        : '${Platform.environment['HOME'] ?? '/tmp'}/.local/share';
    return Directory('$base/rabbiz/apps/$appId');
  }

  Future<void> writeJson(String name, Map<String, Object?> data) async {
    final dir = dataDir();
    await dir.create(recursive: true);
    final target = File('${dir.path}/$name.json');
    final tmp = File('${dir.path}/.$name.json.tmp');
    await tmp.writeAsString(const JsonEncoder.withIndent('  ').convert(data));
    await tmp.rename(target.path);
  }
}

typedef _AbiVersionNative = ffi.Uint32 Function();
typedef _AbiVersionDart = int Function();

/// FFI loader for `librabbiz_sdk_c` with automatic pure-Dart fallback.
class RabbizSdkBindings {
  final ffi.DynamicLibrary? _lib;

  RabbizSdkBindings._(this._lib);

  static RabbizSdkBindings open({String? libraryPath}) {
    try {
      if (libraryPath != null) {
        return RabbizSdkBindings._(ffi.DynamicLibrary.open(libraryPath));
      }
      final libName = Platform.isMacOS
          ? 'librabbiz_sdk_c.dylib'
          : 'librabbiz_sdk_c.so';
      return RabbizSdkBindings._(ffi.DynamicLibrary.open(libName));
    } catch (_) {
      return RabbizSdkBindings._(null);
    }
  }

  bool get isNativeLoaded => _lib != null;

  int abiVersion() {
    final lib = _lib;
    if (lib == null) return 1;
    final fn = lib.lookupFunction<_AbiVersionNative, _AbiVersionDart>(
      'rabbiz_sdk_abi_version',
    );
    return fn();
  }

  RabbizTheme loadTheme({bool dark = false}) {
    return dark ? RabbizTheme.plumDark : RabbizTheme.creamGlassLight;
  }
}
