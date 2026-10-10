import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:go_router/go_router.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/auth.dart';
import 'package:meno_mobile/features/onboarding/onboarding.dart';
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
}

void _registerCore(Config config) {
  di
    ..registerSingleton<Config>(config)
    ..registerSingleton<MenoLogger>(MenoLoggerImpl(config.env))
    ..registerSingleton<FlutterSecureStorage>(const FlutterSecureStorage())
    // Registered asynchronously so the app's startup gate waits for the
    // preference cache to load; every read after that is synchronous.
    ..registerSingletonAsync<SharedPreferences>(SharedPreferences.getInstance)
    ..registerLazySingleton<LocalStorage>(
      () => LocalStorage(di<SharedPreferences>()),
      dispose: (storage) => storage.onDispose(),
    )
    ..registerLazySingleton<SecureStorage>(
      () => SecureStorage(di<FlutterSecureStorage>()),
      dispose: (storage) => storage.onDispose(),
    )
    // The accounts on this device, and the active one among them.
    ..registerLazySingleton<SessionStore>(
      () => SessionStore(di<SecureStorage>()),
      dispose: (store) => store.onDispose(),
    )
    // The client owns the interceptor that attaches — and renews — the active
    // account's token, so no feature has to think about authentication.
    ..registerLazySingleton<HttpClient>(
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
    )
    ..registerLazySingleton<OnboardingService>(
      () => OnboardingService(di<LocalStorage>()),
    )
    ..registerLazySingleton<OnboardingManager>(
      () => OnboardingManager(di<OnboardingService>()),
      dispose: (manager) => manager.onDispose(),
    )
    ..registerLazySingleton<AuthHttpService>(
      () => AuthHttpService(di<HttpClient>()),
    )
    // Async, because startup waits for the stored account to be restored: the
    // router must not build against "nobody is signed in" and then navigate
    // away from the sign-in screen a frame later.
    //
    // `dependsOn` is deliberately not used to spell out that ordering: it lists
    // other *async* singletons to wait for, and the service and the store are
    // lazy ones that exist the moment they are asked for. Naming them would
    // stop the container from ever becoming ready.
    ..registerSingletonAsync<AuthManager>(
      () => AuthManager(
        http: di<AuthHttpService>(),
        store: di<SessionStore>(),
        logger: di<MenoLogger>(),
      ).init(),
      dispose: (manager) => manager.onDispose(),
    )
    // The read-only view other features take, so they never get the ability to
    // sign an account out.
    ..registerLazySingleton<UserManager>(() => UserManager(di<AuthManager>()))
    // Follows the active account and keeps the authenticated scope in step.
    ..registerSingletonAsync<UserScopeManager>(
      () => UserScopeManager(di<AuthManager>()).init(),
      dependsOn: [AuthManager],
      dispose: (manager) => manager.onDispose(),
    )
    ..registerLazySingleton<GoRouter>(_createRouter);
}

GoRouter _createRouter() {
  return createRouter(
    auth: di<AuthManager>(),
    onboarding: di<OnboardingManager>(),
    debugLogDiagnostics: di<Config>().env.isDev,
  );
}
