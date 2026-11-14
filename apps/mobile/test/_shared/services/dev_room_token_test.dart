import 'dart:convert';

import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_shared/services/dev_room_token.dart';

/// The token's three sections, decoded back to JSON.
({Map<String, dynamic> header, Map<String, dynamic> claims}) _decode(
  String token,
) {
  final parts = token.split('.');
  expect(parts, hasLength(3), reason: 'a JWT is header.payload.signature');

  Map<String, dynamic> section(String value) =>
      jsonDecode(utf8.decode(base64Url.decode(base64Url.normalize(value))))
          as Map<String, dynamic>;

  return (header: section(parts[0]), claims: section(parts[1]));
}

void main() {
  const source = DevRoomTokenSource(
    room: 'broadcast-1',
    identity: 'user-1',
    name: 'Ada',
  );

  test('signs what the local LiveKit server validates', () {
    final token = source.token(canPublish: true);
    final decoded = _decode(token);

    expect(decoded.header, {'alg': 'HS256', 'typ': 'JWT'});
    expect(decoded.claims['iss'], DevRoomTokenSource.devApiKey);
    expect(decoded.claims['sub'], 'user-1');
    expect(decoded.claims['name'], 'Ada');

    final now = DateTime.now().millisecondsSinceEpoch ~/ 1000;
    expect(decoded.claims['exp'], greaterThan(now));
    expect(decoded.claims['nbf'], lessThanOrEqualTo(now));
  });

  test('grants the host publishing and the listener only subscribing', () {
    Map<String, dynamic> video(bool canPublish) =>
        _decode(source.token(canPublish: canPublish)).claims['video']
            as Map<String, dynamic>;

    expect(video(true), {
      'roomJoin': true,
      'room': 'broadcast-1',
      'canPublish': true,
      'canSubscribe': true,
      'canPublishData': true,
    });

    final listener = video(false);
    expect(listener['canPublish'], isFalse);
    expect(listener['room'], 'broadcast-1');
  });

  test('the signature is the HMAC the dev server checks', () {
    final token = source.token(canPublish: false);
    final parts = token.split('.');

    final expected = Hmac(
      sha256,
      utf8.encode(DevRoomTokenSource.devApiSecret),
    ).convert(utf8.encode('${parts[0]}.${parts[1]}'));

    expect(parts[2], base64Url.encode(expected.bytes).replaceAll('=', ''));
  });

  test('the identity defaults to the name it is shown by', () {
    const anonymous = DevRoomTokenSource(room: 'r', identity: 'u');
    final claims = _decode(anonymous.token(canPublish: false)).claims;

    expect(claims['sub'], 'u');
    expect(claims['name'], 'u');
  });
}
