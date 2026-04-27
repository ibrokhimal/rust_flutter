import 'package:flutter/material.dart';
import 'package:my_app/src/rust/api/window_monitor.dart';
import 'package:my_app/src/rust/api/types.dart';
import 'package:my_app/src/rust/frb_generated.dart';

Future<void> main() async {
  await RustLib.init();
  // Limit Flutter's decoded-image cache: default is 100 MB, 20 entries is enough
  // for a window-monitor app that shows one icon at a time.
  WidgetsFlutterBinding.ensureInitialized();
  PaintingBinding.instance.imageCache.maximumSize = 20;
  PaintingBinding.instance.imageCache.maximumSizeBytes =
      10 * 1024 * 1024; // 10 MB
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

  @override
  void initState() {
    super.initState();
    _windowStream = watchActiveWindow(pollMs: 400);
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

                  // Icon
                  if (info.iconPng != null)
                    Image.memory(info.iconPng!, width: 48, height: 48)
                  else
                    const Icon(Icons.window, size: 48, color: Colors.grey),

                  const SizedBox(height: 16),

                  // Title
                  Text(
                    info.title.isEmpty ? "Noma'lum" : info.title,
                    textAlign: TextAlign.center,
                    style: const TextStyle(
                      fontSize: 20,
                      fontWeight: FontWeight.bold,
                    ),
                  ),

                  const SizedBox(height: 8),

                  // Process name
                  Text(
                    info.processName,
                    style: const TextStyle(color: Colors.grey),
                  ),

                  Text(
                    info.url ?? "",
                    style: const TextStyle(color: Colors.grey),
                  ),

                  // URL (faqat brauzer bo'lsa)
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
