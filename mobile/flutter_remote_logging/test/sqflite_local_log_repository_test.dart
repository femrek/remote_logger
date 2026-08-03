import 'package:flutter/services.dart';
import 'package:flutter_remote_logging/src/strategies/sqflite_local_log_repository.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:remote_logging/remote_logging.dart';
import 'package:sqflite_common_ffi/sqflite_ffi.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  late SqfliteLocalLogRepository repository;

  setUpAll(() {
    // Initialize FFI for testing
    sqfliteFfiInit();
    databaseFactory = databaseFactoryFfi;

    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
          const MethodChannel('plugins.flutter.io/path_provider'),
          (methodCall) async {
            if (methodCall.method == 'getApplicationDocumentsDirectory') {
              return '.';
            }
            return null;
          },
        );
  });

  setUp(() async {
    const dbPath = '.';
    const path = '$dbPath/remote_logs.db';
    await deleteDatabase(path);

    repository = SqfliteLocalLogRepository();
    await repository.init();
  });

  test('init creates database and table', () async {
    const dbPath = '.';
    const path = '$dbPath/remote_logs.db';

    // Check if we can open and query it (would throw if table doesn't exist)
    final db = await openDatabase(path);
    final result = await db.query('logs');
    expect(result, isEmpty);
    await db.close();
  });

  test('saveLogs inserts multiple logs successfully', () async {
    final logs = [
      ServerLog(
        id: '1',
        level: 'INFO',
        message: 'Test message 1',
        timestamp: DateTime.now(),
        loggerName: 'test',
        zone: 'root',
        deviceUUID: 'device',
        sessionId: 'session',
      ),
      ServerLog(
        id: '2',
        level: 'ERROR',
        message: 'Test message 2',
        timestamp: DateTime.now(),
        loggerName: 'test',
        zone: 'root',
        deviceUUID: 'device',
        sessionId: 'session',
      ),
    ];

    await repository.saveLogs(logs);

    final unsentLogs = await repository.getUnsentLogs();
    expect(unsentLogs.length, 2);
    expect(unsentLogs.map((e) => e.id).toSet(), {'1', '2'});
  });

  test('saveLogs replaces existing logs with the same id', () async {
    final log1 = ServerLog(
      id: '1',
      level: 'INFO',
      message: 'Original message',
      timestamp: DateTime.now(),
      loggerName: 'test',
      zone: 'root',
      deviceUUID: 'device',
      sessionId: 'session',
    );

    await repository.saveLogs([log1]);

    final log1Updated = ServerLog(
      id: '1',
      level: 'ERROR',
      // changed
      message: 'Updated message',
      // changed
      timestamp: DateTime.now(),
      loggerName: 'test',
      zone: 'root',
      deviceUUID: 'device',
      sessionId: 'session',
    );

    await repository.saveLogs([log1Updated]);

    final unsentLogs = await repository.getUnsentLogs();
    expect(unsentLogs.length, 1);
    expect(unsentLogs.first.id, '1');
    expect(unsentLogs.first.level, 'ERROR');
    expect(unsentLogs.first.message, 'Updated message');
  });

  test('getUnsentLogs retrieves and correctly deserializes logs', () async {
    final log = ServerLog(
      id: 'test_id',
      level: 'WARNING',
      message: 'Warning msg',
      timestamp: DateTime.utc(2023),
      loggerName: 'test',
      zone: 'root',
      deviceUUID: 'device',
      sessionId: 'session',
    );

    await repository.saveLogs([log]);

    final unsentLogs = await repository.getUnsentLogs();
    expect(unsentLogs.length, 1);

    final retrieved = unsentLogs.first;
    expect(retrieved.id, 'test_id');
    expect(retrieved.level, 'WARNING');
    expect(retrieved.message, 'Warning msg');
    expect(retrieved.timestamp, DateTime.utc(2023));
  });

  test('getUnsentLogs returns empty list if no logs exist', () async {
    final unsentLogs = await repository.getUnsentLogs();
    expect(unsentLogs, isEmpty);
  });

  test('removeLogs deletes specified logs', () async {
    final logs = [
      ServerLog(
        id: '1',
        level: 'INFO',
        message: 'm1',
        timestamp: DateTime.now(),
        loggerName: 't',
        zone: 'z',
        deviceUUID: 'd',
        sessionId: 's',
      ),
      ServerLog(
        id: '2',
        level: 'INFO',
        message: 'm2',
        timestamp: DateTime.now(),
        loggerName: 't',
        zone: 'z',
        deviceUUID: 'd',
        sessionId: 's',
      ),
      ServerLog(
        id: '3',
        level: 'INFO',
        message: 'm3',
        timestamp: DateTime.now(),
        loggerName: 't',
        zone: 'z',
        deviceUUID: 'd',
        sessionId: 's',
      ),
    ];

    await repository.saveLogs(logs);

    await repository.removeLogs(['1', '3']);

    final unsentLogs = await repository.getUnsentLogs();
    expect(unsentLogs.length, 1);
    expect(unsentLogs.first.id, '2');
  });
}
