import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/auth.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_mobile/features/onboarding/onboarding.dart';
import 'package:meno_mobile/features/profile/profile.dart';
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
///   popped by [UserScopeManager] as accounts change — see
///   [_registerAccountScope] for what goes in it. A live broadcast's session is
///   pushed on top of it by [LiveScopeManager], so signing out ends the
///   broadcast's services along with the account's.
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
void configureDependencies(Config config) {
  di.pushNewScope(scopeName: Scope.root);

  _registerCore(config);
  _registerPlatformServices();
  _registerOnboarding();
  _registerAuth();
  _registerAppearance();

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
  di.registerSingletonAsync<Database>(Database.create);
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
  di.registerSingletonWithDependencies<PermissionsService>(
    () => PermissionsService(di<LocalStorage>(), di<MenoLogger>()),
    dependsOn: [LocalStorage],
  );
  di.registerLazySingleton<PushNotificationsService>(
    () => PushNotificationsService(.instance, di<MenoLogger>()),
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
  // `dependsOn: [LocalStorage]` is what the account's own services rest on: the
  // profile and settings caches, and the record of a live session, are all read
  // and written through the device's store, and they are created eagerly when
  // this scope is pushed.
  di.registerSingletonAsync<UserScopeManager>(
    () => UserScopeManager(
      di<AuthManager>(),
      logger: di<MenoLogger>(),
      register: _registerAccountScope,
    ).init(),
    dependsOn: [AuthManager, LocalStorage],
    dispose: (manager) => manager.onDispose(),
  );
}

/// Registers what belongs to one account, in that account's scope.
///
/// Everything registered here is created when the account becomes active and
/// disposed together when it is switched or signed out, which is the whole
/// point of the tier: the socket that is authenticated as this account, the
/// caches that are keyed by it, the managers that write its name into what they
/// save, and the room it may be in.
///
/// Nothing here is reachable while nobody is signed in — the router sends every
/// signed-out visitor to a sign-in screen — so registering it later costs a
/// frame nothing and buys a lifetime that ends where it should.
Future<void> _registerAccountScope(
  GetIt scope,
  UserCredential credential,
) async {
  // The container disposes all of this: dropping the scope asks everything
  // registered in it that implements `Disposable` to tear down, and the
  // managers below all do. The explicit `dispose` on the lazy registrations is
  // what get_it would otherwise fall back to — an instance it creates itself is
  // only disposed through `Disposable` when there is one.
  //
  // The socket belongs to the account: its handshake is authenticated with the
  // access token this credential carries, so it is created when an account
  // becomes active and thrown away with it. Registering it in the root scope
  // would leave one signed-in user's socket live for the next one.
  scope.registerLazySingleton<SocketClient>(
    () => SocketClient(
      url: di<Config>().webSocketUrl,
      token: credential.session.accessToken,
      logger: di<MenoLogger>(),
    ),
    dispose: (client) => client.onDispose(),
  );

  scope.registerLazySingleton<BroadcastSocketService>(
    () => BroadcastSocketService(
      client: di<SocketClient>(),
      logger: di<MenoLogger>(),
    ),
    dispose: (service) => service.onDispose(),
  );

  // Nothing of its own: every call carries the session already. It is here
  // because nothing that uses it is reachable without an account.
  scope.registerLazySingleton<BroadcastHttpService>(
    () => BroadcastHttpService(di<HttpClient>()),
  );

  di.registerSingletonWithDependencies<BroadcastLocalService>(
    () => BroadcastLocalService(di<Database>(), logger: di<MenoLogger>()),
    dependsOn: [Database],
  );

  // The account's own broadcasts. A draft carries the id that wrote it and a
  // favourite carries the id that kept it, so both go with the account; the
  // lobby is where a broadcast is browsed, which an account is what does.
  scope.registerLazySingleton<BroadcastEditorManager>(
    () => BroadcastEditorManager(
      http: di<BroadcastHttpService>(),
      local: di<BroadcastLocalService>(),
      media: di<MediaService>(),
      live: di<LiveScopeManager>(),
      userId: credential.userId,
      logger: di<MenoLogger>(),
    ),
    dispose: (manager) => manager.onDispose(),
  );

  scope.registerLazySingleton<FavouritesManager>(
    () => FavouritesManager(
      local: di<BroadcastLocalService>(),
      userId: credential.userId,
      logger: di<MenoLogger>(),
    ),
    dispose: (manager) => manager.onDispose(),
  );

  scope.registerLazySingleton<StreamManager>(
    () => StreamManager(
      http: di<BroadcastHttpService>(),
      live: di<LiveScopeManager>(),
      userId: credential.userId,
      logger: di<MenoLogger>(),
    ),
    dispose: (manager) => manager.onDispose(),
  );

  // The account's own record, read as soon as it becomes active and cached per
  // account. The repository above it is shared by everything drawing the same
  // account — a card in a broadcast list outlives the page that fetched it — so
  // its lifetime is its holders', and the account's at the outside.
  scope.registerLazySingleton<ProfileHttpService>(
    () => ProfileHttpService(di<HttpClient>()),
  );

  scope.registerLazySingleton<ProfileCacheService>(
    () => ProfileCacheService(di<LocalStorage>()),
  );

  scope.registerLazySingleton<UserProfileRepository>(UserProfileRepository.new);
  // Started, not awaited. `init` puts this account's cached record in place
  // synchronously and reads the API behind it, so the manager is useful the
  // moment it exists — a screen built in the same frame shows the record rather
  // than a spinner — and the scope stays a synchronous push, with nothing
  // registered that is not yet ready to be resolved.
  final profile = MyProfileManager(
    http: di<ProfileHttpService>(),
    cache: di<ProfileCacheService>(),
    user: di<UserManager>(),
    logger: di<MenoLogger>(),
  );
  unawaited(profile.init());
  scope.registerSingleton<MyProfileManager>(profile);

  // The account's switches, and the cache behind them — also per account, since
  // one device holds several. The theme itself stays in the root scope, because
  // it is the account's choice rather than the account's property.
  scope.registerLazySingleton<SettingsHttpService>(
    () => SettingsHttpService(di<HttpClient>()),
  );

  scope.registerLazySingleton<SettingsLocalService>(
    () => SettingsLocalService(di<LocalStorage>()),
  );

  // Started for the same reason, and eagerly rather than on first use: the
  // account's own display choice has to reach the theme as soon as the account
  // is active, not when somebody opens the settings screen.
  final settings = SettingsManager(
    http: di<SettingsHttpService>(),
    local: di<SettingsLocalService>(),
    user: di<UserManager>(),
    theme: di<ThemeManager>(),
    logger: di<MenoLogger>(),
  );
  unawaited(settings.init());
  scope.registerSingleton<SettingsManager>(settings);

  // The live tier sits on top of this one, and is handed the registrar that
  // fills it — so opening a room is `start(broadcastId)` and nothing else has
  // to remember what a session needs. The record of the session this device was
  // in is the account's too: it is the account that would go back to it.
  scope.registerSingleton<LiveSessionStore>(
    LiveSessionStore(di<LocalStorage>(), userId: credential.userId),
  );
  final live = LiveScopeManager(
    credential.userId,
    logger: di<MenoLogger>(),
    store: di<LiveSessionStore>(),
    http: di<BroadcastHttpService>(),
    endedBroadcasts: di<BroadcastSocketService>().onEndedBroadcast,
    register: _registerLiveScope,
  );
  unawaited(live.init());
  scope.registerSingleton<LiveScopeManager>(live);
}

/// Registers one live session's services, in that session's own scope.
///
/// Everything here is created when a room opens and disposed when it closes:
/// the room's transport, the roster, and the manager that owns the room's
/// state.
/// The [LiveSession] the scope is opened for is registered by
/// [LiveScopeManager] before this runs, which is why the registrations below
/// can take it.
///
/// # One of them needs a token, and a room may not have one yet
///
/// Entering a room is `POST …/join` or `…/live`, and the answer is the only
/// thing that carries a LiveKit token. A scope opened from a broadcast id alone
/// — a deep link that has not joined yet — registers the room's client and its
/// roster but not the session manager, because a session manager without a
/// token could only fabricate a connection.
/// `di.isRegistered<LiveSessionManager>()` is the question a caller asks before
/// entering.
Future<void> _registerLiveScope(GetIt scope, LiveSession session) async {
  scope.registerSingleton<ParticipantsManager>(
    ParticipantsManager(
      http: di<BroadcastHttpService>(),
      socket: di<BroadcastSocketService>(),
      broadcastId: session.broadcastId,
    ),
    dispose: (manager) => manager.onDispose(),
  );

  // The room itself. Which server it opens, and where its token comes from, is
  // the one difference between a development build and a release one:
  //
  // * a release build joins [Config.menoLiveKitUrl] with the token the API
  //   minted for the session;
  // * a development build joins the LiveKit `ops/docker-compose.yml` starts,
  //   over the loopback address `adb reverse` forwards, and signs its own
  //   token with the development key pair that server runs with — so going
  //   live on a device costs nothing and needs no internet.
  //
  // A room server that is not configured at all is walked on the silent
  // transport: the flow still runs, and nothing is published.
  final config = di<Config>();
  final roomUrl = config.liveKitUrl;
  if (roomUrl.isEmpty) {
    di<MenoLogger>().w(
      'No LiveKit URL is configured; the broadcast will run without a room',
      tag: MenoLogTag.di.label,
    );
    scope.registerLazySingleton<RoomTransport>(
      SilentRoomTransport.new,
      dispose: (transport) => transport.onDispose(),
    );
  } else {
    // Lazy: the SDK's room is built when a session enters the room, not when a
    // scope is opened from a deep link that may never join one.
    scope.registerLazySingleton<RoomTransport>(
      () => LiveKitRoomTransport(
        url: roomUrl,
        logger: di<MenoLogger>(),
        devToken: config.usesLocalRoomServer
            ? DevRoomTokenSource(
                room: session.broadcastId.getOrCrash,
                identity: session.userId.getOrCrash,
              )
            : null,
      ),
      dispose: (transport) => transport.onDispose(),
    );
  }

  if (!scope.isRegistered<BroadcastSession>()) return;

  scope.registerSingleton<LiveSessionManager>(
    LiveSessionManager(
      session: di<BroadcastSession>(),
      http: di<BroadcastHttpService>(),
      local: di<BroadcastLocalService>(),
      socket: di<BroadcastSocketService>(),
      room: di<RoomTransport>(),
      live: di<LiveScopeManager>(),
      logger: di<MenoLogger>(),
    ),
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
    auth: di<AuthManager>(),
    onboarding: di<OnboardingManager>(),
    debugLogDiagnostics: di<Config>().env.isDev,
  );
}
