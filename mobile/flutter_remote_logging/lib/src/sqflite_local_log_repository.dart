import 'dart:convert';
import 'package:path/path.dart';
import 'package:path_provider/path_provider.dart';
import 'package:remote_logging/remote_logging.dart';
import 'package:sqflite/sqflite.dart';

/// An SQLite implementation of [ILocalLogRepository] to store unsent logs.
class SqfliteLocalLogRepository implements ILocalLogRepository {
  Database? _db;

  /// Initializes the SQLite database.
  Future<void> init() async {
    final docsDir = await getApplicationDocumentsDirectory();
    final dbPath = join(docsDir.path, 'remote_logs.db');

    _db = await openDatabase(
      dbPath,
      version: 1,
      onCreate: (db, version) async {
        await db.execute('''
          CREATE TABLE logs (
            id TEXT PRIMARY KEY,
            payload TEXT
          )
        ''');
      },
    );
  }

  @override
  Future<void> saveLogs(List<ServerLog> logs) async {
    if (_db == null) return;

    final batch = _db!.batch();
    for (final log in logs) {
      batch.insert(
        'logs',
        {
          'id': log.id,
          'payload': jsonEncode(log.toMap()),
        },
        conflictAlgorithm: ConflictAlgorithm.replace,
      );
    }
    await batch.commit(noResult: true);
  }

  @override
  Future<List<ServerLog>> getUnsentLogs() async {
    if (_db == null) return [];

    final maps = await _db!.query('logs');
    final logs = <ServerLog>[];

    for (final map in maps) {
      final payload = map['payload'] as String?;
      if (payload != null) {
        try {
          final json = jsonDecode(payload) as Map<String, dynamic>;
          logs.add(ServerLog.fromJson(json));
        } on Object catch (e, st) {
          localLog.warning('Failed to parse saved log payload', e, st);
        }
      }
    }

    return logs;
  }

  @override
  Future<void> removeLogs(List<String> logIds) async {
    if (_db == null || logIds.isEmpty) return;

    // SQFlite doesn't easily support arbitrary length IN clauses due to
    // variable binding limits (e.g., 999 max on Android SQLite). Since
    // batches are small (e.g. 50), it is usually fine, but to be safe, we
    // can use a batch delete.
    final batch = _db!.batch();
    for (final id in logIds) {
      batch.delete(
        'logs',
        where: 'id = ?',
        whereArgs: [id],
      );
    }
    await batch.commit(noResult: true);
  }
}
