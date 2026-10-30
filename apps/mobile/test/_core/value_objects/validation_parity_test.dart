import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';

/// The one accepted/rejected policy file both languages read.
///
/// `apps/api/tests/auth_validators.rs` reads the same JSON, so a case added for
/// one side and not honoured by the other makes one of the two suites fail
/// (audit H25). Paths are relative to the package root, which is the working
/// directory `flutter test` runs in.
Map<String, dynamic> _fixture() {
  final file = File('../../fixtures/auth-validation.json');
  if (!file.existsSync()) {
    fail('The shared auth-validation fixture is missing: ${file.path}');
  }
  final decoded = jsonDecode(file.readAsStringSync());
  if (decoded is! Map<String, dynamic>) {
    fail('The shared auth-validation fixture is not a JSON object');
  }
  return decoded;
}

/// Whether the mobile policy accepts [value] as [kind].
///
/// A rejection is a `ValueObject` holding a `Left`, which is what the forms
/// read — not an exception, so a rejected value never throws here.
bool _accepts(String kind, String value) => switch (kind) {
  'email' => Email(value).isValid,
  // Registration is where the server enforces the password policy.
  'password' => Password.register(value).isValid,
  'full_name' => FullName(value).isValid,
  _ => fail('Unknown case kind "$kind" in the shared fixture'),
};

List<Map<String, dynamic>> _cases(Map<String, dynamic> fixture, String side) =>
    (fixture[side] as List).cast<Map<String, dynamic>>();

void main() {
  final fixture = _fixture();

  test('the fixture exercises every field on both sides', () {
    const kinds = {'email', 'password', 'full_name'};
    for (final side in ['accepted', 'rejected']) {
      expect(
        {for (final case_ in _cases(fixture, side)) case_['kind']},
        containsAll(kinds),
        reason: 'the "$side" list must cover every kind of value',
      );
    }
  });

  group('the server accepts these, so the client must too', () {
    for (final case_ in _cases(fixture, 'accepted')) {
      final kind = case_['kind'] as String;
      final value = case_['value'] as String;

      test('$kind — ${case_['why']}', () {
        expect(_accepts(kind, value), isTrue, reason: case_['why'] as String);
      });
    }
  });

  group('the server rejects these, so the client must too', () {
    for (final case_ in _cases(fixture, 'rejected')) {
      final kind = case_['kind'] as String;
      final value = case_['value'] as String;

      test('$kind — ${case_['why']}', () {
        expect(_accepts(kind, value), isFalse, reason: case_['why'] as String);
      });
    }
  });

  test('signing in checks presence only, never the composition policy', () {
    // A password the API accepted at registration can never be refused at the
    // sign-in form, even if it breaks a rule the registration form enforces.
    expect(Password.login('abcdefgh').isValid, isTrue);
    expect(Password.register('abcdefgh').isValid, isFalse);
  });
}
