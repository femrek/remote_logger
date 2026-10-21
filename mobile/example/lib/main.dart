import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:remote_logging_example/app.dart';

void main() {
  WidgetsFlutterBinding.ensureInitialized();

  FlutterError.onError = (FlutterErrorDetails details) {
    FlutterError.presentError(details);
    debugPrint('[FLUTTER_FRAMEWORK_ERROR] ${details.exceptionAsString()}');
    if (details.stack != null) {
      debugPrint('[FLUTTER_FRAMEWORK_ERROR] Stack: ${details.stack}');
    }
  };

  PlatformDispatcher.instance.onError = (Object error, StackTrace stack) {
    debugPrint('[PLATFORM_ASYNC_ERROR] Uncaught async error: $error');
    debugPrint('[PLATFORM_ASYNC_ERROR] Stack: $stack');
    return true;
  };

  runApp(const RemoteLoggerExampleApp());
}
