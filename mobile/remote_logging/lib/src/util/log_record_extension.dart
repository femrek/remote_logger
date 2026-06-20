import 'dart:async';

import 'package:logging/logging.dart';

/// Extension on [LogRecord] to provide additional functionality.
extension LogRecordExtension on LogRecord {
  /// Retrieves the name of the zone associated with the log record.
  String get zoneName {
    if (zone == null) return 'no-zone';
    if (identical(Zone.root, zone)) return 'root';
    final name = zone?['name'];
    return name is String && name.isNotEmpty ? name : 'unnamed-zone';
  }
}
