import 'package:remote_logging/remote_logging.dart';

/// A no-op implementation of [LocalLogRepository] that does not store
/// or persist logs locally.
final class NoSaveLocalLogRepository implements LocalLogRepository {
  /// Creates a [NoSaveLocalLogRepository].
  const NoSaveLocalLogRepository();

  @override
  Future<List<ServerLog>> getUnsentLogs() {
    return Future.value([]);
  }

  @override
  Future<void> removeLogs(List<String> logIds) {
    return Future.value();
  }

  @override
  Future<void> saveLogs(List<ServerLog> logs) {
    return Future.value();
  }
}
