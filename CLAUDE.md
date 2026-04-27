# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this project is

A Flutter desktop app that monitors the currently active OS window (title, process name, app icon, and browser URL) by calling native platform APIs through a Rust library bridged via `flutter_rust_bridge` v2.

## Commands

### Flutter
```sh
flutter pub get          # install Dart dependencies
flutter run              # run on connected device/desktop
flutter test             # unit tests (lib/test/)
flutter test integration_test/simple_test.dart  # integration test (requires device)
flutter build macos      # build macOS release
```

### Rust
```sh
cd rust
cargo build              # compile the Rust library
cargo test               # run Rust unit tests
cargo clippy             # lint
```

### Bridge code generation
Run whenever the Rust `crate::api` surface changes:
```sh
flutter_rust_bridge_codegen generate
```

## Architecture

```
lib/main.dart                    Flutter UI — consumes WindowInfo stream
lib/src/rust/                    Auto-generated Dart bindings (do NOT edit)
  frb_generated.dart             RustLib.init() — must be called before runApp()
  api/window_monitor.dart        watchActiveWindow(pollMs:) Dart stream
  api/types.dart                 WindowInfo Dart class

rust/src/
  lib.rs                         Crate root; declares api, platform modules
  frb_generated.rs               Auto-generated bridge glue (do NOT edit)
  api/
    types.rs                     WindowInfo struct, MonitorError enum
    window_monitor.rs            current_window() + watch_active_window() (StreamSink)
  platform/
    mod.rs                       #[cfg] dispatch to OS-specific impl
    macos.rs                     Implemented: NSWorkspace + CGWindowListCopyWindowInfo
    windows.rs                   TODO stub
    linux.rs                     TODO stub (X11)

rust_builder/                    Flutter FFI plugin package — glue only, do not edit
flutter_rust_bridge.yaml         Bridge config: rust_input + dart_output paths
```

### Data flow
1. `RustLib.init()` loads the compiled Rust `.dylib`/`.so`/`.dll` at startup.
2. Flutter calls `watchActiveWindow(pollMs: 400)` which returns a `Stream<WindowInfo>`.
3. On the Rust side, a Tokio loop polls `platform::current_window()` every `poll_ms` ms and pushes changed `WindowInfo` values into the `StreamSink`.
4. The `StreamBuilder` in `main.dart` renders each emitted `WindowInfo`.

### Adding new Rust API
1. Add public `async fn` or struct in `rust/src/api/` (only items in `crate::api` are exported).
2. Run `flutter_rust_bridge_codegen generate` to regenerate `lib/src/rust/` and `rust/src/frb_generated.rs`.
3. Import the generated Dart file in Flutter and call it.

### Platform implementation notes
- macOS is the only fully implemented platform. Windows and Linux stubs return `Ok(None)`.
- macOS uses `objc2` + `objc2-app-kit` for `NSWorkspace` and `core-graphics` for `CGWindowListCopyWindowInfo` to get the window title.
- `watch_active_window` only pushes to the sink when `WindowInfo` changes (equality check) to avoid unnecessary Flutter rebuilds.
- `icon_png` is `Option<Vec<u8>>` — raw PNG bytes suitable for `Image.memory()` in Flutter.
