import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/auth.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_mobile/features/profile/profile.dart';
import 'package:meno_mobile/features/settings/settings.dart';

/// Registers what belongs to one account, in that account's scope.
///
/// Everything registered here is created when the account becomes active and
/// disposed together when it is switched or signed out, which is the whole
/// point of the tier: the socket that is authenticated as this account, the
/// caches that are keyed by it, the managers that write its name into what they
/// save, and the room it may be in.
///
/// There is exactly one live owner — [ActiveBroadcastManager] — and its session
/// is an ordinary object it creates and closes itself. No scope is pushed for a
/// room, so "is it registered?" is never asked to mean "is it on air?".
///
/// Nothing here is reachable while nobody is signed in — the router sends every
/// signed-out visitor to a sign-in screen — so registering it later costs a
/// frame nothing and buys a lifetime that ends where it should.
Future<void> registerAccountScope(
  GetIt scope,
  CredentialRecord credential,
  AccountContext context,
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
  //
  // The token is a supplier, not a value: the client reads it on every connect
  // and reconnect, so a reconnect after a refresh presents the new token and
  // nothing here has to notice that it changed.
  //
  // The supplier is a read, not a value: it asks the connection owner, whose
  // `accessToken` reads the committed credential at the moment of the
  // handshake, so a reconnect after a refresh presents the new token and an
  // account switch can never reuse the old one (audit M03).
  //
  // The owner is resolved lazily rather than captured — it is registered after
  // this client, and asking for it here is only ever done at connect time.
  Future<String?> currentAccessToken() =>
      di<AccountConnectionOwner>().accessToken();

  scope.registerLazySingleton<SocketClient>(
    () => SocketClient(
      uri: Uri.parse(di<Config>().webSocketUrl),
      credentialSupplier: currentAccessToken,
      logger: di<MenoLogger>(),
      clock: di<Clock>(),
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

  // Created here rather than through `dependsOn`: an account is only ever
  // scoped after the root container is ready, so the store it needs exists, and
  // a dependency-based registration that waits across scopes makes get_it
  // complete its ready future twice (`Future already completed`).
  final local = BroadcastLocalService(
    context.accountId,
    di<Database>(),
    logger: di<MenoLogger>(),
  );
  scope.registerSingleton<BroadcastLocalService>(local);

  // A database written before the unique logical keys existed can hold two rows
  // for one draft or favourite. Clearing that up is part of becoming
  // owner-bound (audit M06, assumption A13); it is best-effort, so a store
  // that cannot be scanned is retried on the next launch rather than failing
  // sign-in.
  unawaited(
    local.dedupe().catchError((Object error) {
      di<MenoLogger>().w(
        'Could not clean up duplicate local rows',
        tag: MenoLogTag.lifecycle.label,
        error: error,
      );
    }),
  );

  // The account's own broadcasts. A draft carries the id that wrote it and a
  // favourite carries the id that kept it, so both go with the account; the
  // editor itself is created by its page, not here (Step 34).
  scope.registerLazySingleton<FavouritesManager>(
    () => FavouritesManager(
      local: di<BroadcastLocalService>(),
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
    // Viewer-bound: every cached profile is keyed by who is looking, so two
    // accounts never share what they saw of the same subject.
    () => ProfileCacheService(context.accountId, di<LocalStorage>()),
  );

  // // Subject-only profile entries from older builds belong to no particular
  // // viewer; they are deleted rather than read (audit H14, assumption A13).
  // unawaited(di<ProfileCacheService>().purgeLegacyEntries());

  scope.registerLazySingleton<UserProfileRepository>(UserProfileRepository.new);
  // Awaiting the local half is what makes `ready` mean something: when the
  // owner publishes ready, this account's cached record — or the placeholder
  // standing in for it — is already in place, and nothing reads a manager that
  // is still starting. The remote read behind it is started by the account
  // subtree's first widget, not awaited here (Step 26).
  final profile = MyProfileManager(
    context: context,
    http: di<ProfileHttpService>(),
    cache: di<ProfileCacheService>(),
    logger: di<MenoLogger>(),
  );
  await profile.init();
  scope.registerSingleton<MyProfileManager>(profile);

  // The account's switches, and the cache behind them — also per account, since
  // one device holds several. The theme itself stays in the root scope, because
  // it is the account's choice rather than the account's property.
  scope.registerLazySingleton<SettingsHttpService>(
    () => SettingsHttpService(di<HttpClient>()),
  );

  scope.registerLazySingleton<SettingsLocalService>(
    () => SettingsLocalService(context.accountId, di<LocalStorage>()),
  );

  // Eager for the same reason: the account's own display choice has to reach
  // the theme as soon as the account is active, not when somebody opens the
  // settings screen. Awaiting its local install before returning is what puts
  // the account's theme in place before `ready`.
  final settings = SettingsManager(
    context: context,
    http: di<SettingsHttpService>(),
    local: di<SettingsLocalService>(),
    theme: di<ThemeManager>(),
    logger: di<MenoLogger>(),
  );
  await settings.init();
  scope.registerSingleton<SettingsManager>(settings);

  // Delivery starts here and nowhere else: the adapters' handlers are attached
  // before the socket is asked to connect, and everything is stopped before the
  // scope takes the socket away. Registered after the socket, so it is disposed
  // before it.
  final connection = AccountConnectionOwner(
    context: context,
    socket: di<SocketClient>(),
    adapters: [di<BroadcastSocketService>()],
    credentials: di<SessionService>(),
    logger: di<MenoLogger>(),
    clock: di<Clock>(),
  );
  scope.registerSingleton<AccountConnectionOwner>(connection);
  connection.initialize();
  unawaited(connection.start(verified: credential.isVerified));

  // The record of the session this device was in is the account's too: it is
  // the account that would go back to it.
  scope.registerSingleton<LiveSessionStore>(
    LiveSessionStore(di<LocalStorage>(), userId: credential.accountId),
  );

  // The account's one live owner. Its session is built by the factory below —
  // an ordinary object, created and closed by the manager — so nothing about a
  // room is registered in the container.
  scope.registerSingleton<ActiveBroadcastManager>(
    ActiveBroadcastManager(
      context: context,
      gateway: BroadcastLiveGateway(di<BroadcastHttpService>()),
      store: di<LiveSessionStore>(),
      logger: di<MenoLogger>(),
      // A host-end or a forced end arrives on the socket: the manager ends the
      // room once from here rather than waiting for the room to be taken away.
      endedBroadcasts: di<BroadcastSocketService>().onEndedBroadcast,
      createSession: ({required session, required exitOnServer}) {
        final participants = ParticipantsManager(
          http: di<BroadcastHttpService>(),
          socket: di<BroadcastSocketService>(),
          broadcastId: session.broadcast.id,
        );
        return ActiveBroadcastSession(
          session: session,
          room: _createRoomTransport(session.broadcast.id, session.userId),
          roster: ParticipantsRoster(participants),
          participants: participants,
          clock: di<Clock>(),
          logger: di<MenoLogger>(),
          exitOnServer: exitOnServer,
        );
      },
    ),
  );

  // A process that died inside a room comes back to it. Queued behind nothing
  // else yet, and refused once an account exit has begun.
  unawaited(di<ActiveBroadcastManager>().recover());
}

/// The room transport one session opens.
///
/// Which server it opens, and where its token comes from, is the one difference
/// between a development build and a release one:
///
/// * a release build joins [Config.menoLiveKitUrl] with the token the API
///   minted for the session;
/// * a development build joins the LiveKit `ops/docker-compose.yml` starts,
///   over the loopback address `adb reverse` forwards, and signs its own
///   token with the development key pair that server runs with — so going
///   live on a device costs nothing and needs no internet.
///
/// Which transport is not guessed: the silent one is reachable only through the
/// explicit [Config.useFakeRoomTransport] flag, which validation already
/// refuses in production. Everything else gets the real one — an unconfigured
/// room URL becomes a visible failure when the session opens rather than a
/// broadcast that silently goes nowhere (M18).
RoomTransport _createRoomTransport(Id broadcastId, Id userId) {
  final config = di<Config>();
  if (config.useFakeRoomTransport && !config.env.isProd) {
    di<MenoLogger>().w(
      'The fake room transport was selected by configuration',
      tag: MenoLogTag.di.label,
    );
    return SilentRoomTransport();
  }

  return LiveKitRoomTransport(
    url: config.liveKitUrl,
    logger: di<MenoLogger>(),
    devToken: config.usesLocalRoomServer
        ? DevRoomTokenSource(
            room: broadcastId.getOrCrash,
            identity: userId.getOrCrash,
          )
        : null,
  );
}
