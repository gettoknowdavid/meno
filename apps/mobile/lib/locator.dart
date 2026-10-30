import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/app/account_transition_owner.dart';
import 'package:meno_mobile/app/composition/account_registrations.dart';
import 'package:meno_mobile/features/auth/auth.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_mobile/features/onboarding/onboarding.dart';
import 'package:meno_mobile/features/settings/settings.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// The app's service locator.
///
/// Registered in two layers, and the split is deliberate:
///
/// * [Scope.root] holds everything that outlives an account — the log, the
///   client, storage, the auth manager, the theme. These exist before anybody
///   signs in and must survive signing out.
/// * [Scope.authenticated] holds what belongs to one account, and is pushed and
///   popped by [AccountTransitionOwner] as accounts change — see
///   [registerAccountScope] for what goes in it. A live broadcast is an
///   ordinary object owned by [ActiveBroadcastManager] inside that scope, so
///   signing out closes the room along with the account.
///
/// The test for which layer something belongs in is not "does it need a
/// session" but "is there anything sensible it could do for a device nobody is
/// signed into". The sign-in screens, the onboarding flow, the theme and the
/// device's own storage answer yes; a profile, a settings page, a socket
/// authenticated as someone or a room's services answer no, and they are built
/// when an account becomes active rather than at launch and never after.
final GetIt di = .instance;

/// Registers the root scope.
///
/// Called once from `main()`, before `runApp`: every registration here is
/// either synchronous or an async one the app's startup gate waits for.
///
/// Nothing here passes a `dispose:` callback. Everything with something to tear
/// down implements `Disposable`, and get_it calls `onDispose()` itself when the
/// scope goes away. Passing a function as well is an assertion failure when the
/// instance is given eagerly, and is silently ignored on a lazy or async one
/// (get_it checks `instance is Disposable` first), so registration is one
/// statement per service and the teardown lives in the service.
void configureDependencies(Config config, {HttpClient? httpClient}) {
  di.pushNewScope(scopeName: Scope.root);

  _registerCore(config, httpClient: httpClient);
  _registerPlatformServices();
  _registerOnboarding();
  _registerAuth();
  _registerAppearance();

  di.registerSingleton<PendingDestinationSlot>(PendingDestinationSlot());
  di.registerLazySingleton<GoRouter>(_createRouter);
}

void _registerCore(Config config, {HttpClient? httpClient}) {
  di.registerSingleton<Config>(config);
  di.registerSingleton<Clock>(const SystemClock());
  di.registerSingleton<MenoLogger>(MenoLoggerImpl(config.env));
  di.registerSingleton<FlutterSecureStorage>(const FlutterSecureStorage());
  di.registerLazySingleton<SecureStorage>(
    () => SecureStorage(di<FlutterSecureStorage>()),
    // onCreated: (storage) async => await storage.deleteAll(),
  );
  di.registerSingletonAsync<Database>(Database.create);
  di.registerSingletonAsync<SharedPreferences>(SharedPreferences.getInstance);
  // Async rather than `registerSingletonWithDependencies`: get_it completes the
  // ready future of a dependency-based registration twice when another
  // dependency-based one waits on it, which throws `Future already completed`
  // through `allReady`. Awaiting the preferences here keeps a single chain.
  di.registerSingletonAsync<LocalStorage>(
    () async => LocalStorage(await di.getAsync<SharedPreferences>()),
  );
  if (httpClient != null) {
    di.registerSingleton<HttpClient>(httpClient);
  } else {
    di.registerLazySingleton<HttpClient>(
      () => HttpClient(
        baseUrl: di<Config>().menoApiUrl,
        logger: di<MenoLogger>(),
        interceptors: [
          SessionInterceptor(
            store: di<SessionService>(),
            config: config,
            clock: di<Clock>(),
            logger: di<MenoLogger>(),
          ),
        ],
      ),
    );
  }
  di.registerLazySingleton<MediaService>(MediaService.new);
}

/// Registers the services that wrap a platform capability.
///
/// All three are lazy: nothing is constructed, and no permission is asked for,
/// until a screen that needs them resolves one. That is what keeps a cold start
/// free of prompts.
void _registerPlatformServices() {
  di.registerSingletonWithDependencies<PermissionsService>(
    () => PermissionsService(di<LocalStorage>(), di<MenoLogger>()),
    dependsOn: [LocalStorage],
  );
  di.registerLazySingleton<PushNotificationsService>(
    () => PushNotificationsService(.instance, di<MenoLogger>()),
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
  );
}

void _registerAuth() {
  di.registerLazySingleton<SessionService>(
    () => SessionService(di<SecureStorage>()),
  );
  di.registerLazySingleton<AuthHttpService>(
    () => AuthHttpService(di<HttpClient>()),
  );
  di.registerSingletonAsync<AuthManager>(
    () => AuthManager(
      http: di<AuthHttpService>(),
      store: di<SessionService>(),
      clock: di<Clock>(),
      logger: di<MenoLogger>(),
      beginAttempt: () => di<AccountTransitionOwner>().beginAttempt(),
      adoptCandidate: (credential, {required ticket}) =>
          di<AccountTransitionOwner>().adoptCandidate(
            credential,
            ticket: ticket,
          ),
    ).init(),
  );
  di.registerLazySingleton<UserManager>(() => UserManager(di<AuthManager>()));
  // `dependsOn: [LocalStorage]` is what the account's own services rest on: the
  // profile and settings caches, and the record of a live session, are all read
  // and written through the device's store, and they are created eagerly when
  // this scope is pushed.
  di.registerSingletonAsync<AccountTransitionOwner>(
    () async => AccountTransitionOwner(
      activeRecord: di<AuthManager>().activeRecord,
      logger: di<MenoLogger>(),
      register: registerAccountScope,
      beginLiveExit: () async {
        if (!di.isRegistered<ActiveBroadcastManager>()) return;
        await di<ActiveBroadcastManager>().closeForAccountExit();
      },
      saveCredential: (record) async {
        await di<SessionService>().adoptCandidate(record);
        await di<SessionService>().activate(record.accountId);
      },
    ),
    onCreated: (owner) async => await owner.init(),
    dependsOn: [AuthManager, LocalStorage],
  );
}

/// Registers the appearance the app is painted in.
///
/// The theme is the one setting that is not the account's. It has to be known
/// before the first frame — the sign-in screen is drawn in it too — and both
/// platforms treat appearance as a device preference, so it belongs to the
/// device rather than to whoever is signed in. [SettingsManager] hands it the
/// signed-in account's own choice; signing out leaves the last one standing
/// rather than repainting the app back to the system theme.
void _registerAppearance() {
  // Eager: the theme has to be known before the first frame, and before the
  // account whose settings carry it has been loaded.
  di.registerSingleton<ThemeManager>(ThemeManager());
}

GoRouter _createRouter() {
  return createRouter(
    onboarding: di<OnboardingManager>(),
    owner: di<AccountTransitionOwner>(),
    pending: di<PendingDestinationSlot>(),
    debugLogDiagnostics: di<Config>().env.isDev,
  );
}
