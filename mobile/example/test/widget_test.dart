import 'package:flutter_test/flutter_test.dart';
import 'package:remote_logging_example/app.dart';

void main() {
  testWidgets('Renders Remote Logger Diagnostic Dashboard', (tester) async {
    await tester.pumpWidget(const RemoteLoggerExampleApp());
    await tester.pumpAndSettle();

    // Verify AppBar title
    expect(find.text('Remote Logger Diagnostics'), findsOneWidget);

    // Verify Tab headers
    expect(find.text('Attestation & HW'), findsOneWidget);
    expect(find.text('Log Sender & Console'), findsOneWidget);

    // Verify Configuration elements
    expect(find.text('Gateway URL'), findsOneWidget);
    expect(find.text('Channel ID'), findsOneWidget);
    expect(find.text('Initialize'), findsOneWidget);

    // Verify Hardware tab cards
    expect(find.text('Hardware Security Module'), findsOneWidget);
    expect(find.text('Device Metadata'), findsOneWidget);

    // Scroll down to see Standalone Hardware Diagnostics
    await tester.drag(
      find.text('Hardware Security Module'),
      const Offset(0, -400),
    );
    await tester.pumpAndSettle();
    expect(find.text('Standalone Hardware Diagnostics'), findsOneWidget);

    // Switch to Log Sender & Console tab
    await tester.tap(find.text('Log Sender & Console'));
    await tester.pumpAndSettle();

    // Verify log sender action chips
    expect(find.text('Emit Application Logs'), findsOneWidget);
    expect(find.text('FINE'), findsOneWidget);
    expect(find.text('INFO'), findsOneWidget);
    expect(find.text('WARNING'), findsOneWidget);
    expect(find.text('SEVERE'), findsOneWidget);
    expect(find.text('SHOUT'), findsOneWidget);
  });
}
