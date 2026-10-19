import 'package:flutter/material.dart';
import 'package:remote_logging_example/controllers/diagnostic_controller.dart';
import 'package:remote_logging_example/models/console_log_item.dart';

/// Live color-coded scrollable console widget displaying log events.
class LiveConsoleView extends StatelessWidget {
  /// Creates a [LiveConsoleView].
  const LiveConsoleView({
    required this.controller,
    super.key,
  });

  /// The diagnostic controller.
  final DiagnosticController controller;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final logs = controller.consoleLogs;

    return Column(
      children: [
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 4),
          child: Row(
            children: [
              Text(
                'Live Console (${logs.length} events)',
                style: theme.textTheme.titleSmall?.copyWith(
                  fontWeight: FontWeight.bold,
                ),
              ),
              const Spacer(),
              TextButton.icon(
                icon: const Icon(Icons.clear_all, size: 18),
                label: const Text('Clear'),
                onPressed: controller.clearLogs,
              ),
            ],
          ),
        ),
        Expanded(
          child: Container(
            margin: const EdgeInsets.symmetric(horizontal: 12),
            decoration: BoxDecoration(
              color: theme.colorScheme.surfaceContainerHighest.withValues(
                alpha: 0.25,
              ),
              borderRadius: BorderRadius.circular(8),
              border: Border.all(color: theme.dividerColor),
            ),
            child: logs.isEmpty
                ? const Center(
                    child: Text(
                      'No log events yet. Tap buttons above to emit logs.',
                      style: TextStyle(color: Colors.grey),
                    ),
                  )
                : ListView.builder(
                    controller: controller.logScrollController,
                    itemCount: logs.length,
                    padding: const EdgeInsets.all(8),
                    itemBuilder: (context, index) {
                      return ConsoleLogTile(item: logs[index]);
                    },
                  ),
          ),
        ),
      ],
    );
  }
}

/// Single tile rendering a [ConsoleLogItem] with tag badges and timestamp.
class ConsoleLogTile extends StatelessWidget {
  /// Creates a [ConsoleLogTile].
  const ConsoleLogTile({
    required this.item,
    super.key,
  });

  /// The log item.
  final ConsoleLogItem item;

  Color _getTagColor(String tag) {
    switch (tag) {
      case 'FINE':
        return Colors.blueGrey;
      case 'INFO':
        return Colors.blue;
      case 'WARNING':
        return Colors.orange;
      case 'SEVERE':
      case 'ERROR':
        return Colors.red;
      case 'SHOUT':
        return Colors.purple;
      case 'SUCCESS':
        return Colors.green;
      case 'HARDWARE':
        return Colors.teal;
      default:
        return Colors.grey;
    }
  }

  @override
  Widget build(BuildContext context) {
    final tagColor = _getTagColor(item.tag);
    final timeStr =
        '${item.timestamp.hour.toString().padLeft(2, '0')}:${item.timestamp.minute.toString().padLeft(2, '0')}:${item.timestamp.second.toString().padLeft(2, '0')}';

    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                timeStr,
                style: const TextStyle(
                  fontSize: 11,
                  color: Colors.grey,
                  fontFamily: 'monospace',
                ),
              ),
              const SizedBox(width: 6),
              Container(
                padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 1),
                decoration: BoxDecoration(
                  color: tagColor.withValues(alpha: 0.2),
                  borderRadius: BorderRadius.circular(4),
                ),
                child: Text(
                  item.tag,
                  style: TextStyle(
                    fontSize: 10,
                    fontWeight: FontWeight.bold,
                    color: tagColor,
                  ),
                ),
              ),
              const SizedBox(width: 6),
              Expanded(
                child: Text(
                  item.message,
                  style: const TextStyle(fontSize: 12, fontFamily: 'monospace'),
                ),
              ),
            ],
          ),
          if (item.error != null) ...[
            Padding(
              padding: const EdgeInsets.only(left: 60, top: 2),
              child: Text(
                'Error: ${item.error}',
                style: const TextStyle(
                  fontSize: 11,
                  color: Colors.red,
                  fontFamily: 'monospace',
                ),
              ),
            ),
          ],
        ],
      ),
    );
  }
}
