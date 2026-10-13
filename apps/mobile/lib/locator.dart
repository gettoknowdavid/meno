import 'package:firebase_messaging/firebase_messaging.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/auth.dart';
import 'package:meno_mobile/features/onboarding/onboarding.dart';
import 'package:meno_mobile/features/profile/profile.dart';
import 'package:meno_mobile/features/settings/settings.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// The app's service locator.
///
/// Registered in two layers, and the split is deliberate:
///
/// * [Scope.root] holds everything that outlives an account — the log, the
///   client, storage, the auth manager. These exist before anybody signs in and
///   must survive signing out.
/// * [Scope.authenticated] holds what belongs to one account, and is pushed and
///   popped by [UserScopeManager] as accounts change. Nothing in it has a
///   teardown to remember.
final GetIt di = GetIt.instance;

/// Registers the root scope.
///
/// Called once from `main()`, before `runApp`: every registration here is
/// either synchronous or an async one the app's startup gate waits for.
void configureDependencies(Config config) {
  di.pushNewScope(scopeName: Scope.root);

  _registerCore(config);
  _registerPlatformServices();
  _registerOnboarding();
  _registerAuth();
  _registerProfile();
  _registerSettings();

  di.registerLazySingleton<GoRouter>(_createRouter);
}

void _registerCore(Config config) {
  di.registerSingleton<Config>(config);
  di.registerSingleton<MenoLogger>(MenoLoggerImpl(config.env));
  di.registerSingleton<FlutterSecureStorage>(const FlutterSecureStorage());
  di.registerLazySingleton<SecureStorage>(
    () => SecureStorage(di<FlutterSecureStorage>()),
    dispose: (storage) => storage.onDispose(),
  );
  di.registerSingletonAsync<SharedPreferences>(SharedPreferences.getInstance);
  di.registerSingletonWithDependencies<LocalStorage>(
    () => LocalStorage(di<SharedPreferences>()),
    dispose: (storage) => storage.onDispose(),
    dependsOn: [SharedPreferences],
  );
  di.registerLazySingleton<HttpClient>(
    () => HttpClient(
      baseUrl: di<Config>().menoApiUrl,
      logger: di<MenoLogger>(),
      interceptors: [
        SessionInterceptor(
          store: di<SessionStore>(),
          baseUrl: di<Config>().menoApiUrl,
          logger: di<MenoLogger>(),
        ),
      ],
    ),
    dispose: (client) => client.onDispose(),
  );
  di.registerLazySingleton<MediaService>(MediaService.new);
}

/// Registers the services that wrap a platform capability.
///
/// All three are lazy: nothing is constructed, and no permission is asked for,
/// until a screen that needs them resolves one. That is what keeps a cold start
/// free of prompts.
void _registerPlatformServices() {
  di.registerLazySingleton<PermissionsService>(
    () => PermissionsService(di<MenoLogger>()),
    dispose: (service) => service.onDispose(),
  );
  di.registerLazySingleton<PushNotificationsService>(
    () => PushNotificationsService(
      FirebaseMessaging.instance,
      di<MenoLogger>(),
    ),
    dispose: (service) => service.onDispose(),
  );
  di.registerLazySingleton<LocalNotificationsService>(
    () => LocalNotificationsService(di<MenoLogger>()),
  );
}

void _registerOnboarding() {
  di.registerLazySingleton<OnboardingService>(
    () => OnboardingService(di<LocalStorage>()),
  );
  di.registerLazySingleton<OnboardingManager>(
    () => OnboardingManager(di<OnboardingService>()),
    dispose: (manager) => manager.onDispose(),
  );
}

void _registerAuth() {
  di.registerLazySingleton<SessionStore>(
    () => SessionStore(di<SecureStorage>()),
    dispose: (store) => store.onDispose(),
  );
  di.registerLazySingleton<AuthHttpService>(
    () => AuthHttpService(di<HttpClient>()),
  );
  di.registerSingletonAsync<AuthManager>(
    () => AuthManager(
      http: di<AuthHttpService>(),
      store: di<SessionStore>(),
      logger: di<MenoLogger>(),
    ).init(),
    dispose: (manager) => manager.onDispose(),
  );
  di.registerLazySingleton<UserManager>(() => UserManager(di<AuthManager>()));
  di.registerSingletonAsync<UserScopeManager>(
    () => UserScopeManager(di<AuthManager>()).init(),
    dependsOn: [AuthManager],
    dispose: (manager) => manager.onDispose(),
  );
}

void _registerProfile() {
  di.registerLazySingleton<ProfileHttpService>(
    () => ProfileHttpService(di<HttpClient>()),
  );
  di.registerLazySingleton<ProfileCacheService>(
    () => ProfileCacheService(di<LocalStorage>()),
  );
  di.registerSingletonAsync<MyProfileManager>(
    () => MyProfileManager(
      http: di<ProfileHttpService>(),
      cache: di<ProfileCacheService>(),
      user: di<UserManager>(),
    ).init(),
    dependsOn: [AuthManager],
    dispose: (manager) => manager.onDispose(),
  );
}

void _registerSettings() {
  di.registerLazySingleton<SettingsHttpService>(
    () => SettingsHttpService(di<HttpClient>()),
  );
  di.registerLazySingleton<SettingsLocalService>(
    () => SettingsLocalService(di<LocalStorage>()),
  );
  // Eager: the theme has to be known before the first frame, and before the
  // account whose settings carry it has been loaded.
  di.registerSingleton<ThemeManager>(ThemeManager());
  di.registerSingletonAsync<SettingsManager>(
    () => SettingsManager(
      http: di<SettingsHttpService>(),
      local: di<SettingsLocalService>(),
      user: di<UserManager>(),
      theme: di<ThemeManager>(),
      logger: di<MenoLogger>(),
    ).init(),
    dependsOn: [AuthManager],
    dispose: (manager) => manager.onDispose(),
  );
}

GoRouter _createRouter() {
  return createRouter(
    auth: di<AuthManager>(),
    onboarding: di<OnboardingManager>(),
    debugLogDiagnostics: di<Config>().env.isDev,
  );
}
