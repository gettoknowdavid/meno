import 'package:flutter/material.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/app.dart';
import 'package:meno_mobile/locator.dart';

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  final config = Config(
    menoApiUrl: DevEnv.menoApiUrl,
    webSocketUrl: DevEnv.webSocketUrl,
    menoLiveKitUrl: DevEnv.menoLiveKitUrl,
  );
  configureDependencies(config);
  runApp(const MenoApp());
}
