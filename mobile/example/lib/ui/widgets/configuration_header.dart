import 'package:flutter/material.dart';
import 'package:remote_logging_example/controllers/diagnostic_controller.dart';

/// Top header card containing connection configuration and remote logging control buttons.
class ConfigurationHeader extends StatelessWidget {
  /// Creates a [ConfigurationHeader].
  const ConfigurationHeader({
    required this.controller,
    required this.onShowSnackbar,
    super.key,
  });

  /// The diagnostic controller.
  final DiagnosticController controller;

  /// SnackBar messenger callback.
  final void Function(String message) onShowSnackbar;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final isConnected = controller.isConnected;

    return Card(
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(
                  isConnected
                      ? Icons.check_circle
                      : Icons.radio_button_unchecked,
                  color: isConnected ? Colors.green : Colors.grey,
                  size: 20,
                ),
                const SizedBox(width: 8),
                Text(
                  isConnected
                      ? 'Logger Running'
                      : 'Logger Idle / Uninitialized',
                  style: theme.textTheme.titleMedium?.copyWith(
                    color: isConnected ? Colors.green : Colors.grey[700],
                    fontWeight: FontWeight.bold,
                  ),
                ),
                const Spacer(),
                Switch(
                  value: controller.useHardwareAttestation,
                  onChanged: (val) => controller.useHardwareAttestation = val,
                ),
                Text(
                  controller.useHardwareAttestation ? 'Real HW' : 'Fake HW',
                  style: theme.textTheme.bodySmall,
                ),
              ],
            ),
            const SizedBox(height: 8),
            Row(
              children: [
                Expanded(
                  flex: 3,
                  child: TextField(
                    controller: controller.gatewayUrlController,
                    decoration: const InputDecoration(
                      labelText: 'Gateway URL',
                      isDense: true,
                      border: OutlineInputBorder(),
                    ),
                    style: const TextStyle(fontSize: 13),
                  ),
                ),
                const SizedBox(width: 8),
                Expanded(
                  flex: 2,
                  child: TextField(
                    controller: controller.channelIdController,
                    decoration: const InputDecoration(
                      labelText: 'Channel ID',
                      isDense: true,
                      border: OutlineInputBorder(),
                    ),
                    style: const TextStyle(fontSize: 13),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 6),
            Wrap(
              spacing: 6,
              runSpacing: -8,
              children: [
                ActionChip(
                  label: const Text(
                    'hs-logger.femrek.dev',
                    style: TextStyle(fontSize: 11),
                  ),
                  onPressed: () => controller.gatewayUrlController.text =
                      'https://hs-logger.femrek.dev',
                ),
                ActionChip(
                  label: const Text(
                    '10.0.2.2:8080 (Android)',
                    style: TextStyle(fontSize: 11),
                  ),
                  onPressed: () => controller.gatewayUrlController.text =
                      'http://10.0.2.2:8080',
                ),
                ActionChip(
                  label: const Text(
                    'localhost:8080 (iOS)',
                    style: TextStyle(fontSize: 11),
                  ),
                  onPressed: () => controller.gatewayUrlController.text =
                      'http://localhost:8080',
                ),
              ],
            ),
            const SizedBox(height: 8),
            Row(
              children: [
                ElevatedButton.icon(
                  icon: controller.isInitializing
                      ? const SizedBox(
                          width: 16,
                          height: 16,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        )
                      : const Icon(Icons.play_arrow),
                  label: Text(isConnected ? 'Restart' : 'Initialize'),
                  onPressed: controller.isInitializing
                      ? null
                      : () => controller.initRemoteLogger(
                          onMessage: onShowSnackbar,
                        ),
                ),
                const SizedBox(width: 8),
                if (isConnected) ...[
                  OutlinedButton.icon(
                    icon: const Icon(Icons.stop),
                    label: const Text('Stop'),
                    onPressed: controller.stopRemoteLogger,
                  ),
                  const SizedBox(width: 8),
                  ElevatedButton.icon(
                    icon: controller.isFlushing
                        ? const SizedBox(
                            width: 16,
                            height: 16,
                            child: CircularProgressIndicator(strokeWidth: 2),
                          )
                        : const Icon(Icons.cloud_upload),
                    label: const Text('Flush Now'),
                    style: ElevatedButton.styleFrom(
                      backgroundColor: Colors.teal,
                      foregroundColor: Colors.white,
                    ),
                    onPressed: controller.isFlushing
                        ? null
                        : () => controller.flushLogs(
                            onMessage: onShowSnackbar,
                          ),
                  ),
                ],
              ],
            ),
          ],
        ),
      ),
    );
  }
}
