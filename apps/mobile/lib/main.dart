import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/app.dart';
import 'package:meno_mobile/features/bible/bible.dart';
import 'package:meno_mobile/locator.dart';

/// The only retry guard in the app: a second tap while a retry is running is
/// ignored rather than starting a second container rebuild.
bool _running = false;

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await bootstrap();
}

/// Builds (or rebuilds) the container and mounts the app.
///
/// Called once from [main] and again by the error shell's Retry button. A
/// retry resets get_it with its dispose callbacks first, so the dependency
/// that failed to start is closed and created again rather than awaited a
/// second time — awaiting the same failed future is the defect audit §1.2
/// describes. The `_running` guard makes a double tap run one retry.
Future<void> bootstrap() async {
  if (_running) return;
  _running = true;
  try {
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

    // The retry half: everything registered — and every resource the dispose
    // callbacks own — goes before the registrations are built again.
    if (di.isRegistered<Config>()) {
      await di.reset();
    }

    configureDependencies(config);
    configureGlobalExceptionHandler();

    di<MenoLogger>()
      ..i('App starting', tag: MenoLogTag.lifecycle.label)
      ..event('app_start', {'env': config.env.name});

    runApp(const MenoApp(onRetryBootstrap: bootstrap));

    // After `runApp`, so the first frame is already requested: seeding parses
    // the bundled payloads and writes ~93k verse rows, which must never sit in
    // front of it. Starting it here rather than from a screen is what makes it
    // run on a cold start whether or not anybody signs in; a retry reaches this
    // line again, and a device that is already current finds nothing to do.
    startBibleSeeding();
  } finally {
    _running = false;
  }
}
