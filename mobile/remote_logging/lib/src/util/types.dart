import 'package:logging/logging.dart';

/// Callback type for taking an action when a log record is created.
///
/// Do not use for sending or printing logs; they are already handled by the
/// logging system. This is only for additional actions like analytics or
/// custom processing.
typedef OnLog = void Function(LogRecord record);

/// Callback type for handling background errors during log transmission.
///
/// This is used to report network errors or unexpected exceptions that occur
/// when the logging service is processing and sending log batches.
typedef OnError = void Function(Object error, StackTrace stackTrace);

/// Callback type for formatting and printing a [LogRecord].
typedef LogPrinter = void Function(LogRecord logRecord);
