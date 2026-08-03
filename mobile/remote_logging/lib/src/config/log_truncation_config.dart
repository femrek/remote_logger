/// Configuration for per-field truncation of log messages before transmission.
///
/// Applied at the moment a `LogRecord` is converted to a `ServerLog`, ensuring
/// the queue, local storage, and retry buffer never hold oversized data.
///
/// **Convention:** set any limit to `0` to disable it (unlimited).
///
/// Example — tighter limits for a bandwidth-sensitive context:
/// ```dart
/// const truncation = LogTruncationConfig(
///   maxMessageChars: 2048,
///   maxStacktraceLines: 20,
/// );
/// ```
class LogTruncationConfig {
  /// Creates a [LogTruncationConfig] with optional per-field limits.
  ///
  /// All parameters default to industry-standard values. Set any to `0` to
  /// disable that limit.
  const LogTruncationConfig({
    this.maxMessageChars = 8192,
    this.maxErrorChars = 2048,
    this.maxStacktraceLines = 100,
    this.maxStacktraceLineChars = 512,
  });

  /// Maximum character length of the log message field.
  ///
  /// Messages exceeding this length are truncated and suffixed with
  /// `'...[truncated]'`. Set to `0` for unlimited.
  ///
  /// Default: `8192` (8 KB).
  final int maxMessageChars;

  /// Maximum character length of the error string (`error.toString()`).
  ///
  /// Set to `0` for unlimited.
  ///
  /// Default: `2048` (2 KB).
  final int maxErrorChars;

  /// Maximum number of lines retained from a stacktrace.
  ///
  /// Lines beyond this count are dropped and a `'...[truncated]'` line is
  /// appended. Set to `0` for unlimited.
  ///
  /// Default: `100`.
  final int maxStacktraceLines;

  /// Maximum character length of each individual stacktrace line.
  ///
  /// Lines exceeding this length are truncated and suffixed with
  /// `'...[truncated]'`. Applied after [maxStacktraceLines]. Set to `0` for
  /// unlimited.
  ///
  /// Default: `512`.
  final int maxStacktraceLineChars;

  /// Creates a copy of this config with the given fields replaced.
  LogTruncationConfig copyWith({
    int? maxMessageChars,
    int? maxErrorChars,
    int? maxStacktraceLines,
    int? maxStacktraceLineChars,
  }) {
    return LogTruncationConfig(
      maxMessageChars: maxMessageChars ?? this.maxMessageChars,
      maxErrorChars: maxErrorChars ?? this.maxErrorChars,
      maxStacktraceLines: maxStacktraceLines ?? this.maxStacktraceLines,
      maxStacktraceLineChars:
          maxStacktraceLineChars ?? this.maxStacktraceLineChars,
    );
  }

  @override
  String toString() {
    return 'LogTruncationConfig('
        'maxMessageChars: $maxMessageChars, '
        'maxErrorChars: $maxErrorChars, '
        'maxStacktraceLines: $maxStacktraceLines, '
        'maxStacktraceLineChars: $maxStacktraceLineChars'
        ')';
  }
}
