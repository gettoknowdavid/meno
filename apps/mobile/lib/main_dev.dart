import 'package:flutter/material.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/app.dart';
import 'package:meno_mobile/locator.dart';

void main() {
  WidgetsFlutterBinding.ensureInitialized();

  final config = Config(
    env: Environment.dev,
    menoApiUrl: DevEnv.menoApiUrl,
    webSocketUrl: DevEnv.webSocketUrl,
    menoLiveKitUrl: DevEnv.menoLiveKitUrl,
  );

  configureDependencies(config);

  di<MenoLogger>()
    ..i('App starting', tag: MenoLogTag.lifecycle.label)
    ..event('app_start', {'env': config.env.name});

  runApp(const MenoApp());
}
