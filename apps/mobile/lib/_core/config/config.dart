import 'package:flutter/foundation.dart';
import 'package:meno_mobile/_core/value_objects/id.dart';

@immutable
final class Config {
  const new({
    required this.env,
    required this.menoApiUrl,
    required this.webSocketUrl,
    required this.menoLiveKitUrl,
    this.useFakeRoomTransport = false,
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

  /// Whether the build walks the fake room transport instead of a real server.
  ///
  /// Explicit and false by default: the silent transport is reachable only by
  /// naming it, never by an empty room URL, and [validate] refuses it in
  /// production — a release that shows "live" with no room behind it is the
  /// silent failure M18 describes.
  final bool useFakeRoomTransport;

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

  /// What is wrong with this configuration; empty when nothing is.
  ///
  /// Checked once at startup, before any service is registered: a bad
  /// configuration must stop the app visibly rather than half-work (M18).
  /// Failures name the field, never its value — a URL's query string may carry
  /// a token.
  List<ConfigurationFailure> validate() {
    final failures = <ConfigurationFailure>[];

    final api = Uri.tryParse(menoApiUrl);
    if (api == null || !api.hasScheme || api.host.isEmpty) {
      failures.add(const ConfigurationFailure('menoApiUrl'));
    } else if (!env.isDev && api.scheme != 'https') {
      failures.add(const ConfigurationFailure('menoApiUrl', 'is not https'));
    }

    final socket = Uri.tryParse(webSocketUrl);
    final socketScheme = socket?.scheme;
    final socketOk =
        socket != null &&
        socket.hasScheme &&
        socket.host.isNotEmpty &&
        (socketScheme == 'ws' || socketScheme == 'wss');
    if (!socketOk) {
      failures.add(
        const ConfigurationFailure(
          'webSocketUrl',
          'is not a ws:// or wss:// URL',
        ),
      );
    } else if (!env.isDev && socketScheme != 'wss') {
      failures.add(const ConfigurationFailure('webSocketUrl', 'is not wss'));
    }

    final room = Uri.tryParse(menoLiveKitUrl);
    final roomScheme = room?.scheme;
    final roomOk =
        menoLiveKitUrl.isNotEmpty &&
        room != null &&
        room.hasScheme &&
        room.host.isNotEmpty &&
        (room.scheme == 'ws' || room.scheme == 'wss');
    if (env.isProd) {
      // Production never runs without a room server, and never over a
      // plaintext one.
      if (!roomOk || roomScheme != 'wss') {
        failures.add(
          const ConfigurationFailure('menoLiveKitUrl', 'is missing or not wss'),
        );
      }
      if (useFakeRoomTransport) {
        failures.add(
          const ConfigurationFailure(
            'useFakeRoomTransport',
            'is not allowed in production',
          ),
        );
      }
    } else if (menoLiveKitUrl.isNotEmpty && !roomOk) {
      failures.add(const ConfigurationFailure('menoLiveKitUrl'));
    }

    return failures;
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

/// One field that fails `Config.validate()`, naming the field only — never its
/// value, because a URL can carry a token in its query string.
@immutable
final class ConfigurationFailure {
  const new(this.field, [this.reason = 'is invalid']);

  final String field;
  final String reason;

  @override
  String toString() => '$field $reason';
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
