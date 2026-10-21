import 'package:flutter/material.dart';
import 'package:remote_logging_example/controllers/diagnostic_controller.dart';
import 'package:remote_logging_example/ui/widgets/attestation_tab.dart';
import 'package:remote_logging_example/ui/widgets/configuration_header.dart';
import 'package:remote_logging_example/ui/widgets/log_sender_tab.dart';

/// Main diagnostic dashboard page housing tabs and configuration controls.
class DiagnosticDashboardPage extends StatefulWidget {
  /// Creates a [DiagnosticDashboardPage].
  const DiagnosticDashboardPage({super.key});

  @override
  State<DiagnosticDashboardPage> createState() =>
      _DiagnosticDashboardPageState();
}

class _DiagnosticDashboardPageState extends State<DiagnosticDashboardPage>
    with SingleTickerProviderStateMixin {
  late final TabController _tabController;
  late final DiagnosticController _controller;

  @override
  void initState() {
    super.initState();
    _tabController = TabController(length: 2, vsync: this);
    _controller = DiagnosticController();
  }

  @override
  void dispose() {
    _tabController.dispose();
    _controller.dispose();
    super.dispose();
  }

  void _showSnackbar(String text) {
    if (!mounted) return;
    ScaffoldMessenger.of(context).hideCurrentSnackBar();
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(
        content: Text(text),
        duration: const Duration(seconds: 10),
        behavior: SnackBarBehavior.floating,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: _controller,
      builder: (context, _) {
        return Scaffold(
          appBar: AppBar(
            title: const Text('Remote Logger Diagnostics'),
            elevation: 2,
            actions: [
              IconButton(
                icon: const Icon(Icons.refresh),
                tooltip: 'Refresh Hardware & Token Info',
                onPressed: _controller.isAttestationBusy
                    ? null
                    : _controller.checkHardwareSupportAndMetadata,
              ),
            ],
            bottom: TabBar(
              controller: _tabController,
              tabs: const [
                Tab(icon: Icon(Icons.security), text: 'Attestation & HW'),
                Tab(icon: Icon(Icons.list_alt), text: 'Log Sender & Console'),
              ],
            ),
          ),
          body: Column(
            children: [
              ConfigurationHeader(
                controller: _controller,
                onShowSnackbar: _showSnackbar,
              ),
              Expanded(
                child: TabBarView(
                  controller: _tabController,
                  children: [
                    AttestationTab(
                      controller: _controller,
                      onShowSnackbar: _showSnackbar,
                    ),
                    LogSenderTab(
                      controller: _controller,
                      onShowSnackbar: _showSnackbar,
                    ),
                  ],
                ),
              ),
            ],
          ),
        );
      },
    );
  }
}
