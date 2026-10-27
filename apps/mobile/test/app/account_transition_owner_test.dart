import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/app/account_transition_owner.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';
import 'package:mocktail/mocktail.dart';

const _a = '018f2b1e-0000-7000-8000-000000000001';
const _b = '018f2b1e-0000-7000-8000-000000000002';

class _MockLogger extends Mock implements MenoLogger;

UserCredential _account(String id, {String token = 'access-token'}) {
  return AuthResponseDto.fromJson({
    'access_token': token,
    'refresh_token': 'refresh-token',
    'expires_in': 3600,
    'user': {
      'id': id,
      'full_name': 'Account $id',
      'email': '$id@example.com',
      'verified': true,
    },
  }).toDomain;
}

final class _ScopedService {
  const new(this.userId);

  final Id userId;
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  late ValueNotifier<UserCredential> credential;
  late AccountTransitionOwner owner;

  setUp(() async {
    await di.reset();
    di.pushNewScope(scopeName: Scope.root);
    credential = ValueNotifier(UserCredential.empty);
  });

  tearDown(() async {
    credential.dispose();
    await di.reset();
  });

  test('two activations run one after the other, never interleaved', () async {
    final order = <String>[];
    final firstHeld = Completer<void>();
    owner = AccountTransitionOwner(
      activeCredential: credential,
      logger: _MockLogger(),
      register: (scope, account) async {
        order.add(account.userId.getOrCrash);
        if (account.userId.getOrCrash == _a) await firstHeld.future;
        scope.registerSingleton<_ScopedService>(_ScopedService(account.userId));
      },
    );
    await owner.init();
    await owner.restore();

    credential.value = _account(_a);
    await Future<void>.delayed(const Duration(milliseconds: 10));
    credential.value = _account(_b);
    await Future<void>.delayed(const Duration(milliseconds: 10));

    expect(order, [_a], reason: 'the second must wait for the first');

    firstHeld.complete();
    await owner.ready;

    expect(order, [_a, _b]);
    expect(owner.snapshot.value.context?.accountId, Id.fromString(_b));
    expect(owner.snapshot.value.isReady, isTrue);
  });

  test('a registration failure leaves no scope and blocks', () async {
    owner = AccountTransitionOwner(
      activeCredential: credential,
      logger: _MockLogger(),
      register: (_, _) => throw const NetworkException(),
    );
    await owner.init();
    await owner.restore();

    credential.value = _account(_a);
    await owner.ready;

    expect(owner.snapshot.value.phase, AccountPhase.blocked);
    expect(owner.snapshot.value.failure?.reason,
        BlockedReason.registrationFailed);
    expect(di.hasScope(Scope.authenticated), isFalse);
  });

  test('a slow adoption superseded by a newer intent is cancelled', () async {
    var registrations = 0;
    owner = AccountTransitionOwner(
      activeCredential: credential,
      logger: _MockLogger(),
      register: (scope, account) {
        registrations++;
        scope.registerSingleton<_ScopedService>(_ScopedService(account.userId));
      },
    );
    await owner.init();
    await owner.restore();

    final ticket = owner.beginAttempt();
    owner.beginAttempt();

    final result = await owner.adoptCandidate(_account(_a), ticket: ticket);

    expect(result, isA<TransitionCancelled>());
    expect(registrations, 0, reason: 'a stale candidate writes nothing');
    expect(di.hasScope(Scope.authenticated), isFalse);
    expect(owner.snapshot.value.phase, AccountPhase.signedOut);
  });

  test('re-entering the same account gets a new generation', () async {
    owner = AccountTransitionOwner(
      activeCredential: credential,
      logger: _MockLogger(),
      register: (scope, account) {
        scope.registerSingleton<_ScopedService>(_ScopedService(account.userId));
      },
    );
    await owner.init();
    await owner.restore();

    credential.value = _account(_a);
    await owner.ready;
    final first = owner.snapshot.value.context!;

    credential.value = UserCredential.empty;
    await owner.ready;
    credential.value = _account(_a);
    await owner.ready;
    final second = owner.snapshot.value.context!;

    expect(second.accountId, first.accountId);
    expect(second.generation, greaterThan(first.generation));
    expect(second, isNot(first));
    expect(
      AccountContext(accountId: Id.fromString(_a), generation: 4),
      AccountContext(accountId: Id.fromString(_a), generation: 4),
    );
  });
}
