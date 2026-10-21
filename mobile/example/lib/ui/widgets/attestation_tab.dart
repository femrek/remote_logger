import 'package:flutter/material.dart';
import 'package:remote_logging_example/controllers/diagnostic_controller.dart';
import 'package:remote_logging_example/ui/widgets/info_row.dart';

/// Tab for inspecting hardware security status, device metadata, and running
/// standalone hardware attestation tests.
class AttestationTab extends StatelessWidget {
  /// Creates an [AttestationTab].
  const AttestationTab({
    required this.controller,
    required this.onShowSnackbar,
    super.key,
  });

  /// The diagnostic controller.
  final DiagnosticController controller;

  /// SnackBar messenger callback.
  final void Function(String message) onShowSnackbar;

  Future<void> _confirmAndReset(BuildContext context) async {
    final confirm = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Reset Hardware Key & Token?'),
        content: const Text(
          'This will delete the enrolled key alias from Android KeyStore / iOS Keychain '
          'and delete the stored enrollment token.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('Cancel'),
          ),
          ElevatedButton(
            onPressed: () => Navigator.pop(ctx, true),
            style: ElevatedButton.styleFrom(
              backgroundColor: Theme.of(context).colorScheme.error,
              foregroundColor: Theme.of(context).colorScheme.onError,
            ),
            child: const Text('Reset'),
          ),
        ],
      ),
    );

    if (confirm == true) {
      await controller.resetHardware(onMessage: onShowSnackbar);
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final hardwareSupported = controller.hardwareSupported;
    final checkError = controller.hardwareCheckError;
    final isBusy = controller.isAttestationBusy;
    final metadata = controller.deviceMetadata;
    final storedToken = controller.storedEnrollmentToken;
    final claims = controller.decodedTokenClaims;

    final String statusLabel;
    final Color statusColor;
    if (isBusy) {
      statusLabel = 'Checking...';
      statusColor = Colors.blue;
    } else if (checkError != null) {
      statusLabel = 'ERROR';
      statusColor = Colors.red;
    } else if (hardwareSupported == true) {
      statusLabel = 'SUPPORTED';
      statusColor = Colors.green;
    } else {
      statusLabel = 'UNSUPPORTED';
      statusColor = Colors.orange;
    }

    return ListView(
      padding: const EdgeInsets.only(bottom: 24),
      children: [
        // 1. Hardware Security Module
        Card(
          child: Padding(
            padding: const EdgeInsets.all(12),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Icon(
                      hardwareSupported == true
                          ? Icons.verified_user
                          : (checkError != null
                                ? Icons.error_outline
                                : Icons.warning_amber),
                      color: statusColor,
                    ),
                    const SizedBox(width: 8),
                    Text(
                      'Hardware Security Module',
                      style: theme.textTheme.titleMedium?.copyWith(
                        fontWeight: FontWeight.bold,
                      ),
                    ),
                    const Spacer(),
                    Chip(
                      label: Text(
                        statusLabel,
                        style: const TextStyle(
                          fontSize: 11,
                          fontWeight: FontWeight.bold,
                        ),
                      ),
                      backgroundColor: statusColor.withValues(alpha: 0.15),
                    ),
                  ],
                ),
                if (checkError != null) ...[
                  const SizedBox(height: 8),
                  Container(
                    padding: const EdgeInsets.all(8),
                    decoration: BoxDecoration(
                      color: Colors.red.withValues(alpha: 0.1),
                      borderRadius: BorderRadius.circular(6),
                      border: Border.all(
                        color: Colors.red.withValues(alpha: 0.3),
                      ),
                    ),
                    child: Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        const Icon(
                          Icons.error_outline,
                          color: Colors.red,
                          size: 16,
                        ),
                        const SizedBox(width: 6),
                        Expanded(
                          child: SelectableText(
                            'Hardware check error: $checkError',
                            style: const TextStyle(
                              fontSize: 11,
                              color: Colors.red,
                              fontFamily: 'monospace',
                            ),
                          ),
                        ),
                      ],
                    ),
                  ),
                ],
                const SizedBox(height: 8),
                Text(
                  'Android: Hardware KeyStore (StrongBox / TEE) with Key Attestation.\n'
                  'iOS: Secure Enclave P-256 (CryptoKit) with App Attest.',
                  style: theme.textTheme.bodySmall?.copyWith(
                    color: Colors.grey[600],
                  ),
                ),
              ],
            ),
          ),
        ),

        // 2. Device Metadata
        Card(
          child: Padding(
            padding: const EdgeInsets.all(12),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  'Device Metadata',
                  style: theme.textTheme.titleMedium?.copyWith(
                    fontWeight: FontWeight.bold,
                  ),
                ),
                const Divider(),
                if (metadata == null)
                  Text(
                    checkError != null
                        ? 'Metadata unavailable (see error above).'
                        : 'Collecting device metadata...',
                    style: TextStyle(
                      fontStyle: FontStyle.italic,
                      color: Colors.grey[600],
                    ),
                  )
                else ...[
                  InfoRow(
                    label: 'Platform OS',
                    value: '${metadata.os} ${metadata.osVersion}',
                  ),
                  InfoRow(
                    label: 'Model / Brand',
                    value:
                        '${metadata.manufacturer} ${metadata.brand} ${metadata.model}',
                  ),
                  InfoRow(
                    label: 'Physical Device',
                    value: metadata.isPhysicalDevice,
                  ),
                  InfoRow(
                    label: 'App Identifier',
                    value: metadata.identifier,
                  ),
                  InfoRow(
                    label: 'Device UUID',
                    value: metadata.deviceId,
                    copyable: true,
                    onCopied: () => onShowSnackbar('Copied Device UUID'),
                  ),
                  InfoRow(
                    label: 'Session UUID',
                    value: metadata.sessionId,
                    copyable: true,
                    onCopied: () => onShowSnackbar('Copied Session UUID'),
                  ),
                ],
              ],
            ),
          ),
        ),

        // 3. Stored Enrollment Token
        Card(
          child: Padding(
            padding: const EdgeInsets.all(12),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    const Icon(Icons.vpn_key, size: 20),
                    const SizedBox(width: 8),
                    Text(
                      'Stored Enrollment Token',
                      style: theme.textTheme.titleMedium?.copyWith(
                        fontWeight: FontWeight.bold,
                      ),
                    ),
                  ],
                ),
                const Divider(),
                if (storedToken == null)
                  const Text(
                    'No enrollment token in KeyStore / Keychain.\n'
                    'Perform an auth exchange or manual enroll to mint one.',
                    style: TextStyle(
                      color: Colors.grey,
                      fontStyle: FontStyle.italic,
                    ),
                  )
                else ...[
                  SelectableText(
                    storedToken,
                    style: const TextStyle(
                      fontSize: 11,
                      fontFamily: 'monospace',
                    ),
                    maxLines: 3,
                  ),
                  const SizedBox(height: 8),
                  if (claims != null) ...[
                    Text('Decoded Claims:', style: theme.textTheme.labelMedium),
                    Container(
                      margin: const EdgeInsets.only(top: 4),
                      padding: const EdgeInsets.all(8),
                      decoration: BoxDecoration(
                        color: theme.colorScheme.surfaceContainerHighest
                            .withValues(alpha: 0.4),
                        borderRadius: BorderRadius.circular(6),
                      ),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: claims.entries
                            .map(
                              (e) => Text(
                                '${e.key}: ${e.value}',
                                style: const TextStyle(
                                  fontSize: 12,
                                  fontFamily: 'monospace',
                                ),
                              ),
                            )
                            .toList(),
                      ),
                    ),
                  ],
                ],
              ],
            ),
          ),
        ),

        // 4. Standalone Hardware Diagnostics Actions
        Card(
          child: Padding(
            padding: const EdgeInsets.all(12),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  'Standalone Hardware Diagnostics',
                  style: theme.textTheme.titleMedium?.copyWith(
                    fontWeight: FontWeight.bold,
                  ),
                ),
                Text(
                  'Test native Keystore / Secure Enclave hardware methods directly.',
                  style: theme.textTheme.bodySmall?.copyWith(
                    color: Colors.grey[600],
                  ),
                ),
                const SizedBox(height: 12),
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    ElevatedButton.icon(
                      icon: const Icon(Icons.fingerprint),
                      label: const Text('1. Test Enroll'),
                      onPressed: controller.isAttestationBusy
                          ? null
                          : controller.testHardwareEnroll,
                    ),
                    ElevatedButton.icon(
                      icon: const Icon(Icons.draw),
                      label: const Text('2. Test Prove'),
                      onPressed: controller.isAttestationBusy
                          ? null
                          : controller.testHardwareProve,
                    ),
                    OutlinedButton.icon(
                      icon: const Icon(Icons.delete_forever),
                      label: const Text('Reset HW & Token'),
                      style: OutlinedButton.styleFrom(
                        foregroundColor: theme.colorScheme.error,
                      ),
                      onPressed: controller.isAttestationBusy
                          ? null
                          : () => _confirmAndReset(context),
                    ),
                  ],
                ),
                if (controller.lastEnrollResult != null) ...[
                  const SizedBox(height: 12),
                  const Text(
                    'Last Enroll Result:',
                    style: TextStyle(fontWeight: FontWeight.bold),
                  ),
                  Container(
                    margin: const EdgeInsets.only(top: 4),
                    padding: const EdgeInsets.all(8),
                    decoration: BoxDecoration(
                      color: theme.colorScheme.surfaceContainerHighest
                          .withValues(alpha: 0.5),
                      borderRadius: BorderRadius.circular(6),
                    ),
                    child: SelectableText(
                      controller.lastEnrollResult!,
                      style: const TextStyle(
                        fontSize: 11,
                        fontFamily: 'monospace',
                      ),
                    ),
                  ),
                ],
                if (controller.lastProveResult != null) ...[
                  const SizedBox(height: 12),
                  const Text(
                    'Last Prove Result:',
                    style: TextStyle(fontWeight: FontWeight.bold),
                  ),
                  Container(
                    margin: const EdgeInsets.only(top: 4),
                    padding: const EdgeInsets.all(8),
                    decoration: BoxDecoration(
                      color: theme.colorScheme.surfaceContainerHighest
                          .withValues(alpha: 0.5),
                      borderRadius: BorderRadius.circular(6),
                    ),
                    child: SelectableText(
                      controller.lastProveResult!,
                      style: const TextStyle(
                        fontSize: 11,
                        fontFamily: 'monospace',
                      ),
                    ),
                  ),
                ],
              ],
            ),
          ),
        ),
      ],
    );
  }
}
