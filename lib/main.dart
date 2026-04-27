import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:my_app/src/rust/api/window_monitor.dart';
import 'package:my_app/src/rust/api/types.dart';
import 'package:my_app/src/rust/frb_generated.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  // One icon visible at a time; 10 MB is plenty.
  PaintingBinding.instance.imageCache.maximumSize = 20;
  PaintingBinding.instance.imageCache.maximumSizeBytes = 10 * 1024 * 1024;
  runApp(const MyApp());
}

class MyApp extends StatelessWidget {
  const MyApp({super.key});

  @override
  Widget build(BuildContext context) {
    return const MaterialApp(home: ActiveWindowMonitor());
  }
}

class ActiveWindowMonitor extends StatefulWidget {
  const ActiveWindowMonitor({super.key});

  @override
  State<ActiveWindowMonitor> createState() => _ActiveWindowMonitorState();
}

class _ActiveWindowMonitorState extends State<ActiveWindowMonitor> {
  late final Stream<WindowInfo> _windowStream;

  // Reuse the same Uint8List object per process so Flutter's MemoryImage
  // cache key stays stable and the PNG isn't re-decoded on every title change.
  Uint8List? _iconBytes;
  String? _iconProcessPath;

  @override
  void initState() {
    super.initState();
    _windowStream = watchActiveWindow(pollMs: 400);
  }

  Uint8List? _stableIcon(WindowInfo info) {
    if (info.processPath != _iconProcessPath) {
      _iconProcessPath = info.processPath;
      _iconBytes = info.iconPng;
    }
    return _iconBytes;
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text("Oyna Kuzatuvchisi")),
      body: Center(
        child: StreamBuilder<WindowInfo>(
          stream: _windowStream,
          builder: (context, snapshot) {
            if (snapshot.hasError) {
              return Text("Xatolik: ${snapshot.error}");
            }
            if (!snapshot.hasData) {
              return const CircularProgressIndicator();
            }

            final info = snapshot.data!;
            final icon = _stableIcon(info);

            return Padding(
              padding: const EdgeInsets.all(24),
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  const Text(
                    "Hozirgi aktiv oyna:",
                    style: TextStyle(color: Colors.grey),
                  ),
                  const SizedBox(height: 20),

                  // RepaintBoundary prevents the icon layer from being
                  // repainted when only the title or URL text changes.
                  RepaintBoundary(
                    child: icon != null
                        ? Image.memory(
                            icon,
                            width: 48,
                            height: 48,
                            gaplessPlayback: true,
                          )
                        : const Icon(Icons.window, size: 48, color: Colors.grey),
                  ),

                  const SizedBox(height: 16),

                  Text(
                    info.title.isEmpty ? "Noma'lum" : info.title,
                    textAlign: TextAlign.center,
                    style: const TextStyle(
                      fontSize: 20,
                      fontWeight: FontWeight.bold,
                    ),
                  ),

                  const SizedBox(height: 8),

                  Text(
                    info.processName,
                    style: const TextStyle(color: Colors.grey),
                  ),

                  if (info.url != null) ...[
                    const SizedBox(height: 12),
                    SelectableText(
                      info.url!,
                      style: const TextStyle(color: Colors.blue, fontSize: 14),
                    ),
                  ],
                ],
              ),
            );
          },
        ),
      ),
    );
  }
}
