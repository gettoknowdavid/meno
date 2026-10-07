import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:meno_mobile/_core/_core.dart' show Config, Scope;

final GetIt di = GetIt.instance;

void configureDependencies(Config config) {
  di.pushNewScope(scopeName: Scope.root);
  _setupConfig(config);
}

void _setupConfig(Config config) {
  di
    ..registerSingleton<Config>(config)
    ..registerSingleton<FlutterSecureStorage>(const FlutterSecureStorage());
}
