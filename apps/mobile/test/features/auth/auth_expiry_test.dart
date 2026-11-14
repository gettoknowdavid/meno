import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/exceptions/meno_exception.dart';
import 'package:meno_mobile/_core/value_objects/id.dart';
import 'package:meno_mobile/_shared/model/dtos.dart';
import 'package:meno_mobile/_shared/services/secure_storage.dart';
import 'package:meno_mobile/features/auth/model/credential_record.dart';
import 'package:meno_mobile/features/auth/model/dtos.dart';
import 'package:meno_mobile/features/auth/services/session_service.dart';
import 'package:mocktail/mocktail.dart';

import '../../_support/fake_clock.dart';

const _accountId = '2a84d446-555d-4c21-acd0-a13045f23e12';
final _receipt = DateTime.utc(2026, 1, 1, 12);

class _MockPlatform extends Mock implements FlutterSecureStorage;

AuthResponseDto _response({
  int expiresIn = 3600,
  String accessToken = 'access-token',
  String id = _accountId,
}) {
  return AuthResponseDto(
    accessToken: accessToken,
    expiresIn: expiresIn,
    refreshToken: 'refresh-token',
    user: UserDto(
      id: id,
      fullName: 'Alice',
      email: 'alice@example.com',
      verified: true,
    ),
  );
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  late FakeClock clock;
  late _MockPlatform platform;
  late Map<String, String> disk;

  setUp(() {
    clock = FakeClock(_receipt);
    disk = {};
    platform = _MockPlatform();
    when(() => platform.read(key: any(named: 'key')))
        .thenAnswer((invocation) async {
          return disk[invocation.namedArguments[#key]! as String];
        });
    when(
      () => platform.write(
        key: any(named: 'key'),
        value: any(named: 'value'),
      ),
    ).thenAnswer((invocation) async {
      disk[invocation.namedArguments[#key]! as String] =
          invocation.namedArguments[#value]! as String;
    });
  });

  group('receipt', () {
    test('turns the relative lifetime into one absolute instant', () {
      final record = _response().toCredentialRecord(clock);

      expect(record.accessExpiresAt, _receipt.add(const Duration(hours: 1)));
      expect(record.receivedAt, _receipt);
      expect(record.revision, 1);
      expect(record.epoch, isNotEmpty);
      expect(record.renewalState, RenewalState.idle);
    });

    test('rejects a lifetime that cannot describe a session', () {
      expect(
        () => _response(expiresIn: 0).toCredentialRecord(clock),
        throwsA(isA<MalformedResponseException>()),
      );
      expect(
        () => _response(expiresIn: 40000000).toCredentialRecord(clock),
        throwsA(isA<MalformedResponseException>()),
      );
    });

    test('rejects an unusable account id or empty token', () {
      expect(
        () => _response(id: 'not-a-uuid').toCredentialRecord(clock),
        throwsA(isA<MalformedResponseException>()),
      );
      expect(
        () => _response(accessToken: '   ').toCredentialRecord(clock),
        throwsA(isA<MalformedResponseException>()),
      );
    });
  });

  test('a stored expiry never moves', () async {
    final record = _response().toCredentialRecord(clock);
    final service = SessionService(SecureStorage(platform));
    addTearDown(service.close);
    await service.initialize();
    await service.adoptCandidate(record);

    clock.advance(const Duration(days: 2));

    final reloaded = SessionService(SecureStorage(platform));
    addTearDown(reloaded.close);
    await reloaded.initialize();

    expect(
      reloaded
          .readSnapshot()
          .credentials[Id.fromString(_accountId)]
          ?.accessExpiresAt,
      record.accessExpiresAt,
    );
  });

  test('needsRefresh follows the clock across the skew margin', () {
    final record = _response().toCredentialRecord(clock);

    clock.advance(const Duration(minutes: 58));
    expect(record.needsRefresh(clock), isFalse);

    clock.advance(const Duration(minutes: 1, seconds: 30));
    expect(record.needsRefresh(clock), isTrue);

    clock.advance(const Duration(minutes: 10));
    expect(record.needsRefresh(clock), isTrue);
  });

  test('a record without a usable expiry is unusable', () {
    final json = _response().toCredentialRecord(clock).toJson();

    expect(
      CredentialRecord.tryFromJson(
        Map<String, Object?>.from(json)..remove('accessExpiresAt'),
      ),
      isNull,
    );
    expect(
      CredentialRecord.tryFromJson(
        Map<String, Object?>.from(json)
          ..['accessExpiresAt'] = '2030-01-01T00:00:00',
      ),
      isNull,
    );
  });
}
