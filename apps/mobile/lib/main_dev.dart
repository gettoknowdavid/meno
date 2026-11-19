import 'dart:async';

import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/app.dart';
import 'package:meno_mobile/features/bible/bible.dart';
import 'package:meno_mobile/locator.dart';

void main() {
  WidgetsFlutterBinding.ensureInitialized();

  final config = Config(
    env: Environment.dev,
    menoApiUrl: DevEnv.menoApiUrl,
    webSocketUrl: DevEnv.webSocketUrl,
    menoLiveKitUrl: DevEnv.menoLiveKitUrl,
  );

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

  // Same as `main()`: the version lands after the first frame, and the launch
  // event is recorded whether or not anybody signs in.
  unawaited(di<CrashReportingService>().loadAppInfo());
  unawaited(
    di<AnalyticsService>().track('app_start', {'env': config.env.name}),
  );

  runApp(const MenoApp());

  // Same start as `main()`: after the frame is requested, not before it.
  startBibleSeeding();
}
