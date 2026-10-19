import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

/// Reusable display row for labeled metadata with optional copy-to-clipboard action.
class InfoRow extends StatelessWidget {
  /// Creates an [InfoRow].
  const InfoRow({
    required this.label,
    required this.value,
    this.copyable = false,
    this.onCopied,
    super.key,
  });

  /// Label title.
  final String label;

  /// Value string to display.
  final String value;

  /// Whether to show a copy button.
  final bool copyable;

  /// Callback when copied.
  final VoidCallback? onCopied;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 3),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 120,
            child: Text(
              label,
              style: const TextStyle(
                fontWeight: FontWeight.w600,
                fontSize: 12,
                color: Colors.grey,
              ),
            ),
          ),
          Expanded(
            child: SelectableText(
              value,
              style: const TextStyle(fontSize: 12, fontFamily: 'monospace'),
            ),
          ),
          if (copyable)
            IconButton(
              icon: const Icon(Icons.copy, size: 14),
              padding: EdgeInsets.zero,
              constraints: const BoxConstraints(),
              tooltip: 'Copy',
              onPressed: () {
                unawaited(Clipboard.setData(ClipboardData(text: value)));
                onCopied?.call();
              },
            ),
        ],
      ),
    );
  }
}
