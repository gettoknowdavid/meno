import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:go_router/go_router.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/auth/manager/auth_manager.dart';
import 'package:meno_mobile/features/onboarding/manager/onboarding_manager.dart';
import 'package:meno_mobile/features/onboarding/services/onboarding_service.dart';
import 'package:shared_preferences/shared_preferences.dart';

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
    // Registered asynchronously so the app's startup gate waits for the
    // preference cache to load; every read after that is synchronous.
    ..registerSingletonAsync<SharedPreferences>(SharedPreferences.getInstance)
    ..registerLazySingleton<OnboardingService>(
      () => OnboardingService(di<SharedPreferences>()),
    )
    ..registerLazySingleton<OnboardingManager>(
      () => OnboardingManager(di<OnboardingService>()),
      dispose: (manager) => manager.onDispose(),
    )
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
