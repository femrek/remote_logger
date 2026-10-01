import 'package:remote_logging/src/model/server_log.dart';

/// An interface for local log storage to ensure logs are not lost
/// if the network goes offline or the app crashes.
abstract interface class LocalLogRepository {
  /// Save a list of logs to local storage.
  Future<void> saveLogs(List<ServerLog> logs);

  /// Retrieve all stored logs that haven't been successfully sent yet.
  Future<List<ServerLog>> getUnsentLogs();

  /// Remove logs after they have been successfully sent by their IDs.
  Future<void> removeLogs(List<String> logIds);
}
