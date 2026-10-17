import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_shared/model/_model.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';

/// The body `POST /auth/register` and `POST /auth/login` answer with.
///
/// Keys are copied from what the API serialises — `AuthResponse` and
/// `UserResponse` in `apps/api/src/modules/auth/dto.rs` — not from the Dart
/// field names. That distinction is the whole point of this test: snake_case on
/// the wire, camelCase in Dart, and a `JsonKey` that was misspelled as
/// `fullName` made every register and login fail with
/// `MALFORMED_RESPONSE` while every unit test that never parsed a payload
/// stayed green.
void main() {
  const payload = <String, dynamic>{
    'access_token': 'access-token',
    'refresh_token': 'refresh-token',
    'expires_in': 3600,
    'user': {
      'id': '018f2b1e-0000-7000-8000-000000000000',
      'full_name': 'Ada Lovelace',
      'bio': null,
      'email': 'ada@example.com',
      'verified': true,
      'avatar_id': null,
      'avatar_url': 'https://cdn.example.com/avatars/ada.png',
      'providers': ['password', 'google'],
      'role': 'user',
      'created_at': '2026-10-05T20:51:45.911Z',
    },
  };

  test('the auth payload the API sends parses into a credential', () {
    final credential = AuthResponseDto.fromJson(payload).toDomain;

    expect(credential.session.accessToken, 'access-token');
    expect(credential.session.refreshToken, 'refresh-token');
    expect(credential.session.timeUntilExpiry, isNotNull);

    final user = credential.user;
    expect(user.id, '018f2b1e-0000-7000-8000-000000000000');
    expect(user.fullName, 'Ada Lovelace');
    expect(user.email, 'ada@example.com');
    expect(user.verified, isTrue);
    // expect(user.avatarUrl, 'https://cdn.example.com/avatars/ada.png');
    expect(user.providers, [AuthProvider.password, AuthProvider.google]);
    expect(user.role, UserRole.user);
    expect(user.createdAt, DateTime.utc(2026, 10, 5, 20, 51, 45, 911));
  });

  test('a credential round-trips through the vault unchanged', () {
    final credential = AuthResponseDto.fromJson(payload).toDomain;

    // Through the text, exactly as `SessionStore` writes and reads it: the
    // nested DTO is encoded by `JsonEncoder`'s `toJson` convention, so a
    // round trip that skips the string is not the round trip the app makes.
    final restored = AuthResponseDto.fromJson(
      jsonDecode(jsonEncode(credential.toDto.toJson())) as Map<String, dynamic>,
    ).toDomain;

    expect(restored.user.fullName, credential.user.fullName);
    expect(restored.user.email, credential.user.email);
    expect(restored.user.providers, credential.user.providers);
    expect(restored.session.accessToken, credential.session.accessToken);
    expect(restored.session.refreshToken, credential.session.refreshToken);
  });

  test('a payload missing the display name is reported, not guessed', () {
    final broken = Map<String, dynamic>.of(payload);
    broken['user'] = Map<String, dynamic>.of(
      broken['user']! as Map<String, dynamic>,
    )..remove('full_name');

    expect(() => AuthResponseDto.fromJson(broken), throwsA(isA<Error>()));
  });
}
