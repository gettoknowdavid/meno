import 'package:flutter/foundation.dart';
import 'package:meno_mobile/_core/value_objects/id.dart';

@immutable
final class Config {
  const new({
    required this.env,
    required this.menoApiUrl,
    required this.webSocketUrl,
    required this.menoLiveKitUrl,
  });

  /// The LiveKit server a development build always talks to.
  ///
  /// It is the loopback address, which `adb reverse tcp:7880 tcp:7880`
  /// forwards from a device to the `livekit` service in
  /// `ops/docker-compose.yml`. One address for every development target —
  /// phone over USB, emulator, desktop — so starting a broadcast never depends
  /// on which way this machine happens to be reachable today.
  static const String localLiveKitUrl = 'ws://127.0.0.1:7880';

  final Environment env;
  final String menoApiUrl;
  final String webSocketUrl;
  final String menoLiveKitUrl;

  /// The room server this build connects to.
  ///
  /// A development build never uses [menoLiveKitUrl]: going live on a device
  /// is the thing being worked on, and pointing it at the hosted project would
  /// spend its minutes and need the internet for every test. It uses the
  /// docker server on [localLiveKitUrl] instead — see `docs/local-livekit.md`.
  /// A release build uses the URL the environment carries, unchanged.
  String get liveKitUrl => env.isDev ? localLiveKitUrl : menoLiveKitUrl;

  /// Whether the room server is the local docker one, and so whether a room
  /// token has to be signed here rather than by the API.
  bool get usesLocalRoomServer => env.isDev;

  /// The one origin requests may carry a credential to: scheme, host and
  /// effective port, normalised so `https://api.test` and
  /// `https://api.test:443` are the same origin.
  Uri get apiOrigin => _normalise(Uri.parse(menoApiUrl));

  /// Whether [uri] is the approved API origin.
  ///
  /// A production build additionally requires `https`: a plaintext request to
  /// the right host would still put a bearer token on the wire. Loopback over
  /// plain HTTP is only ever the development build's own server.
  bool isApprovedApiOrigin(Uri uri) {
    final origin = apiOrigin;
    if (!env.isDev && origin.scheme != 'https') return false;
    if (!env.isDev && uri.scheme != 'https') return false;

    final candidate = _normalise(uri);
    return candidate.scheme == origin.scheme &&
        candidate.host == origin.host &&
        candidate.port == origin.port;
  }

  /// [uri] reduced to the three parts that make an origin, with the default
  /// port made explicit and the scheme lower-cased.
  static Uri _normalise(Uri uri) => Uri(
    scheme: uri.scheme.toLowerCase(),
    host: uri.host.toLowerCase(),
    port: uri.hasPort
        ? uri.port
        : (uri.scheme.toLowerCase() == 'https' ? 443 : 80),
  );
}

enum Environment {
  dev,
  prod;

  bool get isProd => this == prod;
  bool get isDev => this == dev;
}

final class Scope {
  const new _();

  static const String root = 'root';
  static const String authenticated = 'authenticated';

  /// The prefix of one live broadcast's scope.
  static const String livePrefix = 'live';

  /// The scope the broadcast [broadcastId] is live in.
  ///
  /// Named for the broadcast rather than a single `live` layer so a session
  /// that replaces another cannot inherit the first one's registrations.
  static String liveFor(Id broadcastId) =>
      '$livePrefix:${broadcastId.getOrCrash}';
}
