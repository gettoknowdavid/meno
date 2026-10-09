import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:go_router/go_router.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/auth/manager/auth_manager.dart';
import 'package:meno_mobile/features/onboarding/manager/onboarding_manager.dart';

final GetIt di = GetIt.instance;

void configureDependencies(Config config) {
  di.pushNewScope(scopeName: Scope.root);
  _registerCore(config);
}

void _registerCore(Config config) {
  di
    ..registerSingleton<Config>(config)
    ..registerSingleton<MenoLogger>(MenoLoggerImpl(config.env))
    ..registerSingleton<FlutterSecureStorage>(const FlutterSecureStorage())
    ..registerLazySingleton<OnboardingManager>(OnboardingManager.new)
    ..registerLazySingleton<AuthManager>(AuthManager.new)
    ..registerLazySingleton<GoRouter>(_createRouter);
}

GoRouter _createRouter() {
  return createRouter(
    auth: di<AuthManager>(),
    onboarding: di<OnboardingManager>(),
    debugLogDiagnostics: di<Config>().env.isDev,
  );
}
