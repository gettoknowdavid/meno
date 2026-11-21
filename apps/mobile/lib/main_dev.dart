import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/bootstrap.dart';

/// The dev entry point: `flutter run -t lib/main_dev.dart`.
///
/// The only difference from [main] is the environment, and therefore which
/// compiled `.env` file the URLs come from.
Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();

  await startApp(
    Config(
      env: Environment.dev,
      menoApiUrl: DevEnv.menoApiUrl,
      webSocketUrl: DevEnv.webSocketUrl,
      menoLiveKitUrl: DevEnv.menoLiveKitUrl,
    ),
  );
}
