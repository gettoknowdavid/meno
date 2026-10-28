import 'dart:convert';

import 'package:crypto/crypto.dart';

/// Signs a LiveKit room token for the docker server development runs on.
///
/// # Why this exists
///
/// Every other build gets its room token from the API — `POST …/live` or
/// `…/join` — because minting one is the server's job and the signing key is
/// its secret. A development build is the exception: the broadcast flow it
/// drives is still fed from fixtures, so there is no session with a real token
/// to enter a room with, and pointing the device at the hosted project to get
/// one would spend its minutes and need the internet. What it needs is to hear
/// itself in a room on this machine.
///
/// That server is `livekit-server --dev`, which starts with the published key
/// pair below and validates exactly what the API would sign: the same claims,
/// the same HMAC. So this signs them.
///
/// # Why it is not a credential
///
/// The key pair is the one LiveKit prints in its own documentation and baked
/// into its development mode, so it authenticates nothing outside a server
/// running with `--dev` on the loopback address. A release build is never
/// handed one of these: the live session factory only builds the transport with a
/// [DevRoomTokenSource] in a development environment, and the URL it is given
/// is the local room server. There is no branch in which a production build can
/// mint a token or reach this server.
final class DevRoomTokenSource {
  /// Creates a source for one client in one room.
  ///
  /// [room] is the room's name on the server. The API names a room after the
  /// broadcast's id, so a second development client joining the same broadcast
  /// lands in the same room without anything else to configure; [identity] is
  /// the account, which is what keeps two devices distinguishable in it.
  const new({
    required this.room,
    required this.identity,
    this.name,
    this.lifetime = const Duration(hours: 6),
  });

  /// The key pair `livekit-server --dev` starts with.
  static const String devApiKey = 'devkey';
  static const String devApiSecret = 'secret';

  /// The room's name.
  final String room;

  /// The participant's identity, which the room keys people by.
  final String identity;

  /// What the room shows for this participant; defaults to [identity].
  final String? name;

  /// How long a token is good for, measured from when it is signed.
  ///
  /// Six hours because a broadcast can run long and the transport signs once
  /// per connection; a client still on air after that reconnects with a fresh
  /// one, which is cheaper than refreshing it mid-session.
  final Duration lifetime;

  /// A token for this client, granted [canPublish] or not.
  ///
  /// The grants are the ones the API mints for a host and a listener: both may
  /// join the one room and subscribe, only a host may publish audio, and both
  /// may publish data (the room's own signalling rides on it).
  String token({required bool canPublish}) {
    final issuedAt = DateTime.now().toUtc();
    final claims = <String, Object?>{
      'iss': devApiKey,
      'sub': identity,
      'name': name ?? identity,
      'nbf': _seconds(issuedAt.subtract(const Duration(seconds: 5))),
      'exp': _seconds(issuedAt.add(lifetime)),
      'video': <String, Object?>{
        'roomJoin': true,
        'room': room,
        'canPublish': canPublish,
        'canSubscribe': true,
        'canPublishData': true,
      },
    };

    final header = _segment({'alg': 'HS256', 'typ': 'JWT'});
    final payload = _segment(claims);
    final signature = Hmac(
      sha256,
      utf8.encode(devApiSecret),
    ).convert(utf8.encode('$header.$payload'));

    return '$header.$payload.${_base64Url(signature.bytes)}';
  }

  static int _seconds(DateTime time) =>
      time.millisecondsSinceEpoch ~/ Duration.millisecondsPerSecond;

  /// One base64url-encoded JWT section, unpadded as the format requires.
  static String _segment(Map<String, Object?> json) =>
      _base64Url(utf8.encode(jsonEncode(json)));

  static String _base64Url(List<int> bytes) =>
      base64Url.encode(bytes).replaceAll('=', '');
}
