import 'package:flutter/material.dart';
import 'package:flutter_remote_logging/flutter_remote_logging.dart';
import 'package:remote_logging_example/controllers/diagnostic_controller.dart';
import 'package:remote_logging_example/ui/widgets/live_console_view.dart';

/// Tab for emitting logs at various severity levels and inspecting live console logs.
class LogSenderTab extends StatelessWidget {
  /// Creates a [LogSenderTab].
  const LogSenderTab({
    required this.controller,
    required this.onShowSnackbar,
    super.key,
  });

  /// The diagnostic controller.
  final DiagnosticController controller;

  /// SnackBar messenger callback.
  final void Function(String message) onShowSnackbar;

  void _sendCustomLog() {
    final text = controller.customLogController.text.trim();
    if (text.isNotEmpty) {
      if (!controller.isConnected) {
        controller.addConsoleLog(
          'WARNING',
          'Cannot send log: RemoteLogger is not initialized. Tap Initialize first.',
        );
        onShowSnackbar('Initialize logger first!');
        return;
      }
      controller.emitLog(Level.INFO, text);
      controller.customLogController.clear();
    }
  }

  void _emit(Level level, String msg, {Object? error, StackTrace? st}) {
    if (!controller.isConnected) {
      controller.addConsoleLog(
        'WARNING',
        'Cannot emit ${level.name} log: RemoteLogger is not initialized. Tap Initialize first.',
      );
      onShowSnackbar('Initialize logger first!');
      return;
    }
    controller.emitLog(level, msg, error: error, st: st);
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Column(
      children: [
        Card(
          child: Padding(
            padding: const EdgeInsets.all(12),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  'Emit Application Logs',
                  style: theme.textTheme.titleMedium?.copyWith(
                    fontWeight: FontWeight.bold,
                  ),
                ),
                const SizedBox(height: 8),
                Wrap(
                  spacing: 6,
                  runSpacing: 6,
                  children: [
                    ActionChip(
                      avatar: const Icon(
                        Icons.bug_report,
                        size: 16,
                        color: Colors.blueGrey,
                      ),
                      label: const Text('FINE'),
                      onPressed: () => _emit(
                        Level.FINE,
                        'Fine diagnostic event at ${DateTime.now().toIso8601String()}',
                      ),
                    ),
                    ActionChip(
                      avatar: const Icon(
                        Icons.info,
                        size: 16,
                        color: Colors.blue,
                      ),
                      label: const Text('INFO'),
                      onPressed: () => _emit(
                        Level.INFO,
                        'User viewed dashboard at ${DateTime.now().toIso8601String()}',
                      ),
                    ),
                    ActionChip(
                      avatar: const Icon(
                        Icons.warning,
                        size: 16,
                        color: Colors.orange,
                      ),
                      label: const Text('WARNING'),
                      onPressed: () => _emit(
                        Level.WARNING,
                        'High memory pressure detected at ${DateTime.now().toIso8601String()}',
                      ),
                    ),
                    ActionChip(
                      avatar: const Icon(
                        Icons.error,
                        size: 16,
                        color: Colors.red,
                      ),
                      label: const Text('SEVERE'),
                      onPressed: () {
                        try {
                          throw StateError(
                            'Simulated checkout failure exception',
                          );
                        } on Object catch (e, s) {
                          _emit(
                            Level.SEVERE,
                            'Payment processing crashed',
                            error: e,
                            st: s,
                          );
                        }
                      },
                    ),
                    ActionChip(
                      avatar: const Icon(
                        Icons.campaign,
                        size: 16,
                        color: Colors.purple,
                      ),
                      label: const Text('SHOUT'),
                      onPressed: () => _emit(
                        Level.SHOUT,
                        'CRITICAL: Security boundary breached!',
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: 8),
                Row(
                  children: [
                    Expanded(
                      child: TextField(
                        controller: controller.customLogController,
                        decoration: const InputDecoration(
                          hintText: 'Custom message...',
                          isDense: true,
                          border: OutlineInputBorder(),
                        ),
                        onSubmitted: (_) => _sendCustomLog(),
                      ),
                    ),
                    const SizedBox(width: 8),
                    ElevatedButton(
                      onPressed: _sendCustomLog,
                      child: const Text('Send'),
                    ),
                  ],
                ),
              ],
            ),
          ),
        ),
        Expanded(
          child: LiveConsoleView(controller: controller),
        ),
        const SizedBox(height: 12),
      ],
    );
  }
}
