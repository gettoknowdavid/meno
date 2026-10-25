import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/value_objects/id.dart';
import 'package:meno_mobile/features/auth/model/credential_record.dart';

void main() {
  group('CredentialRecord', () {
    test('round-trip equality', () {
      final record = CredentialRecord(
        accountId: Id.fromString('2a84d446-555d-4c21-acd0-a13045f23e12'),
        schemaVersion: CredentialRecord.schemaVersionNumber,
        epoch: 'epoch-1',
        revision: 3,
        accessToken: 'access-token',
        refreshToken: 'refresh-token',
        accessExpiresAt: DateTime.utc(2030),
        receivedAt: DateTime.utc(2025),
        isVerified: true,
        renewalState: RenewalState.idle,
      );

      final json = record.toJson();
      final parsed = CredentialRecord.tryFromJson(json);
      expect(parsed, isNotNull);
      expect(parsed!.toJson(), equals(json));
      expect(parsed, equals(record));
    });

    group('tryFromJson returns null for malformed input', () {
      test('non-map input', () {
        expect(CredentialRecord.tryFromJson('not-a-map'), isNull);
        expect(CredentialRecord.tryFromJson(42), isNull);
        expect(CredentialRecord.tryFromJson(null), isNull);
      });

      test('wrong schema version', () {
        final map = <String, Object?>{
          'accountId': '2a84d446-555d-4c21-acd0-a13045f23e12',
          'schemaVersion': 999,
          'epoch': 'epoch',
          'revision': 1,
          'accessToken': 'access',
          'refreshToken': 'refresh',
          'accessExpiresAt': '2030-01-01T00:00:00.000Z',
          'receivedAt': '2025-01-01T00:00:00.000Z',
          'isVerified': true,
          'renewalState': 'idle',
        };
        expect(CredentialRecord.tryFromJson(map), isNull);
      });

      test('invalid account id', () {
        final map = <String, Object?>{
          'accountId': null,
          'schemaVersion': CredentialRecord.schemaVersionNumber,
          'epoch': 'epoch',
          'revision': 1,
          'accessToken': 'access',
          'refreshToken': 'refresh',
          'accessExpiresAt': '2030-01-01T00:00:00.000Z',
          'receivedAt': '2025-01-01T00:00:00.000Z',
          'isVerified': true,
          'renewalState': 'idle',
        };
        expect(CredentialRecord.tryFromJson(map), isNull);
      });

      test('invalid expiry', () {
        final map = <String, Object?>{
          'accountId': '2a84d446-555d-4c21-acd0-a13045f23e12',
          'schemaVersion': CredentialRecord.schemaVersionNumber,
          'epoch': 'epoch',
          'revision': 1,
          'accessToken': 'access',
          'refreshToken': 'refresh',
          'accessExpiresAt': 'not-a-date',
          'receivedAt': '2025-01-01T00:00:00.000Z',
          'isVerified': true,
          'renewalState': 'idle',
        };
        expect(CredentialRecord.tryFromJson(map), isNull);
      });

      test('empty epoch', () {
        final map = <String, Object?>{
          'accountId': '2a84d446-555d-4c21-acd0-a13045f23e12',
          'schemaVersion': CredentialRecord.schemaVersionNumber,
          'epoch': '',
          'revision': 1,
          'accessToken': 'access',
          'refreshToken': 'refresh',
          'accessExpiresAt': '2030-01-01T00:00:00.000Z',
          'receivedAt': '2025-01-01T00:00:00.000Z',
          'isVerified': true,
          'renewalState': 'idle',
        };
        expect(CredentialRecord.tryFromJson(map), isNull);
      });

      test('negative revision', () {
        final map = <String, Object?>{
          'accountId': '2a84d446-555d-4c21-acd0-a13045f23e12',
          'schemaVersion': CredentialRecord.schemaVersionNumber,
          'epoch': 'epoch',
          'revision': -1,
          'accessToken': 'access',
          'refreshToken': 'refresh',
          'accessExpiresAt': '2030-01-01T00:00:00.000Z',
          'receivedAt': '2025-01-01T00:00:00.000Z',
          'isVerified': true,
          'renewalState': 'idle',
        };
        expect(CredentialRecord.tryFromJson(map), isNull);
      });

      test('empty access token', () {
        final map = <String, Object?>{
          'accountId': '2a84d446-555d-4c21-acd0-a13045f23e12',
          'schemaVersion': CredentialRecord.schemaVersionNumber,
          'epoch': 'epoch',
          'revision': 1,
          'accessToken': '',
          'refreshToken': 'refresh',
          'accessExpiresAt': '2030-01-01T00:00:00.000Z',
          'receivedAt': '2025-01-01T00:00:00.000Z',
          'isVerified': true,
          'renewalState': 'idle',
        };
        expect(CredentialRecord.tryFromJson(map), isNull);
      });

      test('missing received at', () {
        final map = <String, Object?>{
          'accountId': '2a84d446-555d-4c21-acd0-a13045f23e12',
          'schemaVersion': CredentialRecord.schemaVersionNumber,
          'epoch': 'epoch',
          'revision': 1,
          'accessToken': 'access',
          'refreshToken': 'refresh',
          'accessExpiresAt': '2030-01-01T00:00:00.000Z',
          'receivedAt': 123,
          'isVerified': true,
          'renewalState': 'idle',
        };
        expect(CredentialRecord.tryFromJson(map), isNull);
      });
    });

    group('RememberedIdentity', () {
      test('never serializes a token-shaped field', () {
        final identity = RememberedIdentity(
          accountId: Id.fromString('2a84d446-555d-4c21-acd0-a13045f23e12'),
          displayName: 'Alice',
          lastUsedAt: DateTime.utc(2025),
          requiresAuthentication: false,
          avatarUrl: 'https://example.com/avatar.png',
          email: 'alice@example.com',
        );

        final json = identity.toJson();
        expect(json, isA<Map<String, Object?>>());
        expect(
          json.keys,
          unorderedEquals([
            'accountId',
            'displayName',
            'lastUsedAt',
            'requiresAuthentication',
            'avatarUrl',
            'email',
          ]),
        );
        for (final key in json.keys) {
          expect(
            key.toLowerCase(),
            isNot(startsWith('token')),
            reason: 'key "$key" looks token-shaped',
          );
        }
      });
    });
  });
}
