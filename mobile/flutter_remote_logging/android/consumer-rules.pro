# Keep rules for dependencies used by flutter_remote_logging
-keep class io.flutter.plugins.sharedpreferences.SharedPreferencesApiRegistrar { *; }
-keep class io.flutter.plugins.sharedpreferences.SharedPreferencesAsyncApiRegistrar { *; }
-keep class io.flutter.plugins.sharedpreferences.** { *; }

-dontwarn com.google.android.play.core.**
