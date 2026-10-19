import 'package:flutter/material.dart';
import 'package:remote_logging_example/ui/diagnostic_dashboard_page.dart';

/// Root widget configuring application themes and the main diagnostic screen.
class RemoteLoggerExampleApp extends StatelessWidget {
  /// Creates a [RemoteLoggerExampleApp].
  const RemoteLoggerExampleApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Remote Logger & Attestation',
      debugShowCheckedModeBanner: false,
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(
          seedColor: const Color(0xFF1E88E5),
        ),
        cardTheme: const CardThemeData(
          elevation: 1,
          margin: EdgeInsets.symmetric(horizontal: 12, vertical: 6),
        ),
      ),
      darkTheme: ThemeData(
        colorScheme: ColorScheme.fromSeed(
          seedColor: const Color(0xFF64B5F6),
          brightness: Brightness.dark,
        ),
        cardTheme: const CardThemeData(
          elevation: 1,
          margin: EdgeInsets.symmetric(horizontal: 12, vertical: 6),
        ),
      ),
      home: const DiagnosticDashboardPage(),
    );
  }
}
