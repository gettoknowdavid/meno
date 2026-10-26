import 'package:flutter/services.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/keys/storage_keys.dart';
import 'package:meno_mobile/_core/value_objects/id.dart';
import 'package:meno_mobile/_shared/services/secure_storage.dart';
import 'package:meno_mobile/features/auth/model/credential_record.dart';
import 'package:meno_mobile/features/auth/model/mutation_result.dart';
import 'package:meno_mobile/features/auth/model/refresh_outcome.dart';
import 'package:meno_mobile/features/auth/services/session_service.dart';
import 'package:mocktail/mocktail.dart';

final _idA = Id.fromString('2a84d446-555d-4c21-acd0-a13045f23e12');
final _idB = Id.fromString('8f14e45f-ceea-4e8f-9a3c-1b2d3e4f5a6b');

CredentialRecord _record({
  Id? accountId,
  String epoch = 'epoch-a',
  int revision = 1,
  String accessToken = 'access-1',
  String? refreshToken = 'refresh-1',
  DateTime? accessExpiresAt,
  RenewalState renewalState = RenewalState.idle,
}) {
  return CredentialRecord(
    accountId: accountId ?? _idA,
    schemaVersion: CredentialRecord.schemaVersionNumber,
    epoch: epoch,
    revision: revision,
    accessToken: accessToken,
    refreshToken: refreshToken,
    accessExpiresAt: accessExpiresAt ?? DateTime.utc(2030),
    receivedAt: DateTime.utc(2025),
    isVerified: true,
    renewalState: renewalState,
  );
}

class _MockPlatform extends Mock implements FlutterSecureStorage {}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  late _MockPlatform platform;
  late Map<String, String> disk;
  late SessionService service;

  void stubReads() {
    when(() => platform.read(key: any(named: 'key'))).thenAnswer((
      invocation,
    ) async {
      return disk[invocation.namedArguments[#key]! as String];
    });
  }

  void stubWrites() {
    when(() =>
            platform.write(key: any(named: 'key'), value: any(named: 'value')))
        .thenAnswer((invocation) async {
          disk[invocation.namedArguments[#key]! as String] =
              invocation.namedArguments[#value]! as String;
        });
  }

  setUp(() {
    disk = {};
    platform = _MockPlatform();
    stubReads();
    stubWrites();
    service = SessionService(SecureStorage(platform));
  });

  tearDown(() => service.close());

  group('initialize', () {
    test('starts signed out when nothing is stored', () async {
      await service.initialize();

      final snapshot = service.readSnapshot();
      expect(snapshot.credentials, isEmpty);
      expect(snapshot.remembered, isEmpty);
      expect(snapshot.activeAccountId, isNull);
      expect(snapshot.revision, 0);
    });

    test('treats an unreadable document as signed out', () async {
      disk[StorageKeys.vaultV2] = '{"schemaVersion":2}';

      await service.initialize();

      expect(service.readSnapshot().credentials, isEmpty);
    });

    test('round-trips a committed vault', () async {
      await service.initialize();
      await service.adoptCandidate(_record());

      final reloaded = SessionService(SecureStorage(platform));
      addTearDown(reloaded.close);
      await reloaded.initialize();

      expect(reloaded.usableCredential(_idA), isNotNull);
      expect(reloaded.readSnapshot().credentials[_idA]!.accessToken, 'access-1');
    });
  });

  group('mutations', () {
    setUp(() async {
      await service.initialize();
    });

    test('commits only one of two concurrent conditional replacements', () async {
      await service.adoptCandidate(_record());

      final first = service.conditionalReplace(
        _idA,
        epoch: 'epoch-a',
        fromRevision: 1,
        accessToken: 'access-2',
        accessExpiresAt: DateTime.utc(2031),
      );
      final second = service.conditionalReplace(
        _idA,
        epoch: 'epoch-a',
        fromRevision: 1,
        accessToken: 'access-3',
        accessExpiresAt: DateTime.utc(2031),
      );

      final results = await Future.wait([first, second]);
      expect(results[0], isA<MutationCommitted>());
      expect(results[1], isA<MutationStale>());

      final record = service.readSnapshot().credentials[_idA]!;
      expect(record.accessToken, 'access-2');
      expect(record.revision, 2);
    });

    test('remove queued behind a replacement leaves no record', () async {
      await service.adoptCandidate(_record());
      await service.activate(_idA);

      final replace = service.conditionalReplace(
        _idA,
        epoch: 'epoch-a',
        fromRevision: 1,
        accessToken: 'access-2',
        accessExpiresAt: DateTime.utc(2031),
      );
      final remove = service.removeCredentials(_idA, epoch: 'epoch-a');

      await Future.wait([replace, remove]);

      final snapshot = service.readSnapshot();
      expect(snapshot.credentials, isEmpty);
      expect(snapshot.activeAccountId, isNull);
    });

    test('a failed write leaves memory untouched and the queue usable', () async {
      await service.adoptCandidate(_record());
      final before = service.readSnapshot();

      when(() =>
              platform.write(key: any(named: 'key'), value: any(named: 'value')))
          .thenThrow(PlatformException(code: 'unavailable'));

      expect(await service.activate(_idA), isA<MutationStorageFailed>());
      expect(service.readSnapshot(), equals(before));

      stubWrites();
      expect(await service.activate(_idA), isA<MutationCommitted>());
      expect(service.readSnapshot().activeAccountId, _idA);
    });

    test('expired credentials are offered only while a renewal is possible', () async {
      await service.adoptCandidate(
        _record(accessExpiresAt: DateTime.utc(2020)),
      );
      await service.adoptCandidate(
        _record(
          accountId: _idB,
          accessExpiresAt: DateTime.utc(2020),
          refreshToken: null,
        ),
      );

      expect(service.usableCredential(_idA), isNotNull);
      expect(service.usableCredential(_idB), isNull);
    });

    test('publishes one projection per committed mutation', () async {
      var emissions = 0;
      service.changes.addListener(() => emissions++);

      await service.adoptCandidate(_record());
      await service.adoptCandidate(_record());
      await service.activate(_idA);

      expect(emissions, 2);
      expect(service.changes.value!.revision, 2);
    });

    test('deactivate keeps credentials but clears the active account', () async {
      await service.adoptCandidate(_record());
      await service.activate(_idA);

      expect(await service.deactivate(), isA<MutationCommitted>());

      final snapshot = service.readSnapshot();
      expect(snapshot.activeAccountId, isNull);
      expect(snapshot.credentials[_idA], isNotNull);
    });

    test('logout keeps the greeting identity and requires authentication', () async {
      await service.adoptCandidate(_record());
      await service.activate(_idA);
      await service.updateRememberedIdentity(
        RememberedIdentity(
          accountId: _idA,
          displayName: 'Alice',
          lastUsedAt: DateTime.utc(2025),
          requiresAuthentication: false,
        ),
      );

      expect(
        await service.removeCredentials(_idA, epoch: 'epoch-a'),
        isA<MutationCommitted>(),
      );

      final snapshot = service.readSnapshot();
      expect(snapshot.credentials, isEmpty);
      expect(snapshot.activeAccountId, isNull);
      expect(snapshot.remembered[_idA]!.requiresAuthentication, isTrue);
    });

    test('a logout for a replaced epoch writes nothing', () async {
      await service.adoptCandidate(_record(epoch: 'epoch-new'));

      expect(
        await service.removeCredentials(_idA, epoch: 'epoch-old'),
        isA<MutationStale>(),
      );
      expect(service.readSnapshot().credentials[_idA], isNotNull);
    });
  });

  group('renewal', () {
    setUp(() async {
      await service.initialize();
      await service.adoptCandidate(_record());
    });

    test('uncertain blocks the credential until reauthentication', () async {
      expect(
        await service.markRenewalInFlight(
          _idA,
          epoch: 'epoch-a',
          revision: 1,
        ),
        isA<MutationCommitted>(),
      );
      // The access token is still valid; the renewal itself is single-flight.
      expect(service.usableCredential(_idA), isNotNull);

      expect(
        await service.settleRenewal(
          _idA,
          epoch: 'epoch-a',
          revision: 1,
          outcome: const RefreshUncertain('timeout'),
        ),
        isA<MutationCommitted>(),
      );

      expect(
        service.readSnapshot().credentials[_idA]!.renewalState,
        RenewalState.uncertain,
      );
      expect(service.usableCredential(_idA), isNull);
    });

    test('an in-flight renewal from a killed process becomes uncertain', () async {
      await service.markRenewalInFlight(
        _idA,
        epoch: 'epoch-a',
        revision: 1,
      );

      final restarted = SessionService(SecureStorage(platform));
      addTearDown(restarted.close);
      await restarted.initialize();

      expect(
        restarted.readSnapshot().credentials[_idA]!.renewalState,
        RenewalState.uncertain,
      );
      expect(restarted.usableCredential(_idA), isNull);
    });

    test('unavailable keeps the pair usable', () async {
      await service.markRenewalInFlight(
        _idA,
        epoch: 'epoch-a',
        revision: 1,
      );

      expect(
        await service.settleRenewal(
          _idA,
          epoch: 'epoch-a',
          revision: 1,
          outcome: const RefreshUnavailable(),
        ),
        isA<MutationCommitted>(),
      );

      expect(service.usableCredential(_idA), isNotNull);
      expect(
        service.readSnapshot().credentials[_idA]!.renewalState,
        RenewalState.idle,
      );
    });

    test('success installs the new pair and bumps the record revision', () async {
      await service.markRenewalInFlight(
        _idA,
        epoch: 'epoch-a',
        revision: 1,
      );

      expect(
        await service.settleRenewal(
          _idA,
          epoch: 'epoch-a',
          revision: 1,
          outcome: RefreshSucceeded(
            _record(
              accessToken: 'access-2',
              refreshToken: 'refresh-2',
              accessExpiresAt: DateTime.utc(2031),
            ),
          ),
        ),
        isA<MutationCommitted>(),
      );

      final record = service.readSnapshot().credentials[_idA]!;
      expect(record.accessToken, 'access-2');
      expect(record.refreshToken, 'refresh-2');
      expect(record.revision, 2);
    });

    test('a late outcome for an older revision is stale', () async {
      await service.markRenewalInFlight(
        _idA,
        epoch: 'epoch-a',
        revision: 1,
      );
      await service.settleRenewal(
        _idA,
        epoch: 'epoch-a',
        revision: 1,
        outcome: RefreshSucceeded(
          _record(accessToken: 'access-2', accessExpiresAt: DateTime.utc(2031)),
        ),
      );

      expect(
        await service.settleRenewal(
          _idA,
          epoch: 'epoch-a',
          revision: 1,
          outcome: const RefreshUncertain('late'),
        ),
        isA<MutationStale>(),
      );
    });
  });

  test('refuses to serve a snapshot before initialization', () {
    expect(service.readSnapshot, throwsStateError);
  });

  test('refuses mutations after close', () async {
    await service.initialize();
    await service.close();

    final result = await service.activate(_idA);
    expect(result, isA<MutationRejected>());
    expect(
      (result as MutationRejected).reason,
      MutationRejection.closing,
    );
  });

  test('unknown accounts are rejected rather than invented', () async {
    await service.initialize();

    expect(
      await service.activate(_idB),
      isA<MutationRejected>(),
    );
  });
}
