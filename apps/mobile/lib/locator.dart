import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:meno_mobile/_core/_core.dart';

final GetIt di = GetIt.instance;

void configureDependencies(Config config) {
  di.pushNewScope(scopeName: Scope.root);
  _registerCore(config);
}

void _registerCore(Config config) {
  di
    ..registerSingleton<Config>(config)
    ..registerSingleton<MenoLogger>(MenoLoggerImpl(config.env))
    ..registerSingleton<FlutterSecureStorage>(const FlutterSecureStorage());
}
