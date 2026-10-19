/// Represents a single logged event in the live diagnostic console.
class ConsoleLogItem {
  /// Creates a [ConsoleLogItem].
  const ConsoleLogItem({
    required this.timestamp,
    required this.tag,
    required this.message,
    this.error,
    this.stackTrace,
  });

  /// The timestamp when the event occurred.
  final DateTime timestamp;

  /// The category or severity tag (e.g. INFO, SEVERE, HARDWARE, SUCCESS).
  final String tag;

  /// The log or status message.
  final String message;

  /// Optional error details.
  final String? error;

  /// Optional stack trace.
  final String? stackTrace;
}
