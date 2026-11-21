import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/bootstrap.dart';

/// The production entry point: `flutter run`.
///
/// The URLs are compiled in from `.env.prod`; everything after building this
/// config is [startApp], which the dev entry point shares.
Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();

  await startApp(
    Config(
      env: Environment.prod,
      menoApiUrl: ProdEnv.menoApiUrl,
      webSocketUrl: ProdEnv.webSocketUrl,
      menoLiveKitUrl: ProdEnv.menoLiveKitUrl,
    ),
  );
}
