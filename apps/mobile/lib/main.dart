import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/app.dart';
import 'package:meno_mobile/locator.dart';

void main() {
  WidgetsFlutterBinding.ensureInitialized();

  final config = Config(
    env: Environment.prod,
    menoApiUrl: ProdEnv.menoApiUrl,
    webSocketUrl: ProdEnv.webSocketUrl,
    menoLiveKitUrl: ProdEnv.menoLiveKitUrl,
  );

  // A bad production configuration stops here, visibly, instead of falling
  // through to a frame that silently behaves differently (M18).
  final failures = config.validate();
  if (failures.isNotEmpty) {
    runApp(ConfigurationErrorApp(failures: failures));
    return;
  }

  configureDependencies(config);
  configureGlobalExceptionHandler();

  di<MenoLogger>()
    ..i('App starting', tag: MenoLogTag.lifecycle.label)
    ..event('app_start', {'env': config.env.name});

  runApp(const MenoApp());
}
