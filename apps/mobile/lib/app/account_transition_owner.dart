import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';

/// Registers the services that belong to one account, in that account's scope.
typedef AccountScopeRegistrar = FutureOr<void> Function(
  GetIt scope,
  CredentialRecord credential,
  AccountContext context,
);

/// The one owner of account activation and deactivation.
///
/// Every transition — sign-in, switch, sign-out, startup restore — is appended
/// to one FIFO queue and applied by this class alone. No other code pushes or
/// pops [Scope.authenticated], so a slow login cannot land after a logout and
/// two activations cannot interleave.
///
/// The published [snapshot] is the app's readiness signal: `ready` means the
/// account's scope exists and the account is usable; `preparing`, `quiescing`
/// and `blocked` are all states in which a private screen must not build.
final class AccountTransitionOwner implements Disposable {
  new({
    required this._activeRecord,
    required this._logger,
    this.register,
    this.beginLiveExit,
    this.saveCredential,
    this.frameWait = const Duration(milliseconds: 250),
  });

  final ValueListenable<CredentialRecord?> _activeRecord;
  final MenoLogger _logger;
  final AccountScopeRegistrar? register;

  /// Drains and closes the live tier before the account scope comes down.
  final Future<void> Function()? beginLiveExit;

  /// Writes the credential to the vault once its scope exists — the owner is
  /// the only caller, so "stored" and "ready" cannot drift apart.
  final Future<void> Function(CredentialRecord credential)? saveCredential;

  /// How long [exit] waits for the frame that removes the old subtree.
  final Duration frameWait;

  /// The single account readiness observable.
  final ValueNotifier<AccountSnapshot> snapshot = ValueNotifier(
    AccountSnapshot.signedOut,
  );

  Future<void> _queue = Future<void>.value();
  ListenableSubscription? _subscription;
  bool _hasScope = false;
  int _generation = 0;
  int _attemptSerial = 0;
  CredentialRecord? _current;
  Id? _desiredAccountId;
  bool _disposed = false;

  /// The account currently in scope, or null.
  AccountContext? get currentContext => snapshot.value.context;

  /// Starts following the active record.
  Future<AccountTransitionOwner> init() async {
    _subscription = _activeRecord.listen(_onCredentialChanged);
    _desiredAccountId = _accountIdOf(_activeRecord.value);
    return this;
  }

  /// Restores the account that was already signed in.
  Future<void> restore() async {
    final record = _activeRecord.value;
    _desiredAccountId = _accountIdOf(record);

    if (record == null) {
      _publish(AccountSnapshot.signedOut);
      return;
    }

    await _enqueue(() => _prepare(record));
  }

  /// Waits for the transition already in flight.
  Future<void> get ready => _queue;

  /// Starts a new intent and returns its ticket.
  ///
  /// Any ticket issued earlier is superseded; a candidate adopted with a stale
  /// ticket is discarded without writing anything.
  int beginAttempt() => ++_attemptSerial;

  /// Adopts a freshly issued credential if [ticket] is still the newest intent.
  Future<TransitionResult> adoptCandidate(
    CredentialRecord record, {
    required int ticket,
  }) {
    if (ticket != _attemptSerial) {
      return Future.value(const TransitionCancelled());
    }
    // The record's account becomes the desired one so that _prepare's guard
    // passes even when no stored session exists yet (first login).
    _desiredAccountId = record.accountId;
    // An account already in scope is taken down first; two accounts must never
    // overlap (H02).
    final leaving = _hasScope || di.hasScope(Scope.authenticated);
    return _enqueue(() => leaving ? _exit(next: record) : _prepare(record));
  }

  /// Ends the current account and, when [next] is given, prepares it.
  Future<void> exit({CredentialRecord? next}) {
    return _enqueue(() => _exit(next: next));
  }

  /// Invalidates every admitted intent and drains the queue.
  Future<void> shutdown() async {
    _attemptSerial += 1;
    _desiredAccountId = null;
    await _enqueue(_exit);
  }

  @override
  FutureOr<dynamic> onDispose() async {
    if (_disposed) return;
    _disposed = true;
    _attemptSerial += 1;
    _desiredAccountId = null;
    _subscription?.cancel();
    _subscription = null;
    // No frame wait on the way out: the container is already tearing down, and
    // nothing may schedule work against a tree that is going away.
    await _popScope();
    snapshot.dispose();
  }

  void _onCredentialChanged(
    CredentialRecord? record,
    ListenableSubscription _,
  ) {
    final accountId = _accountIdOf(record);
    _desiredAccountId = accountId;

    if (accountId == _current?.accountId) {
      if (record == null) {
        unawaited(_enqueue(_exit));
        return;
      }
      // A renewal, or a verification, is the same account: only the verified
      // flag may change, and the scope (and therefore the subtree key) stays.
      _current = record;
      _publish(
        AccountSnapshot(
          phase: AccountPhase.ready,
          context: snapshot.value.context,
          isEmailVerified: record.isVerified,
        ),
      );
      return;
    }

    unawaited(_enqueue(() => _exit(next: record)));
  }

  /// Runs [operation] after every transition admitted before it.
  Future<T> _enqueue<T>(Future<T> Function() operation) {
    final completer = Completer<T>();
    _queue = _queue.then((_) async {
      try {
        completer.complete(await operation());
      } on Object catch (error, stackTrace) {
        _logger.e(
          'Account transition failed',
          tag: MenoLogTag.di.label,
          error: error,
          stackTrace: stackTrace,
        );
        if (!completer.isCompleted) {
          completer.completeError(error, stackTrace);
        }
      }
    });
    return completer.future;
  }

  Future<TransitionResult> _prepare(CredentialRecord credential) async {
    if (_disposed) {
      return const TransitionCancelled();
    }

    final accountId = credential.accountId;
    if (_desiredAccountId != accountId) {
      return const TransitionCancelled();
    }

    // Already in scope for this account: the vault publishes synchronously,
    // so a restore can arrive right after the change event that already
    // prepared it.
    // Pushing a second scope would leave two of everything for one account.
    final current = snapshot.value.context;
    if (_hasScope && _current?.accountId == accountId && current != null) {
      _current = credential;
      _publish(
        AccountSnapshot(
          phase: AccountPhase.ready,
          context: current,
          isEmailVerified: credential.isVerified,
        ),
      );
      return TransitionReady(current);
    }

    final context = AccountContext(
      accountId: accountId,
      generation: ++_generation,
    );
    _publish(AccountSnapshot(phase: AccountPhase.preparing, context: context));

    try {
      await di.pushNewScopeAsync(
        scopeName: Scope.authenticated,
        init: (scope) async {
          scope.registerSingleton<CredentialRecord>(credential);
          await register?.call(scope, credential, context);
        },
      );

      // The account may have left while the registrar was running. Nothing
      // half-built may be advertised as ready, so cancel and take it down.
      if (_disposed || _desiredAccountId != accountId) {
        await _popScope();
        return const TransitionCancelled();
      }

      _hasScope = true;
      _current = credential;

      // Stored before `ready` is published: a screen that builds on ready
      // issues requests, and those requests need this credential on disk.
      await saveCredential?.call(credential);

      // Anything that reaches the network from here on is the account subtree's
      // first widget (Step 18/19); the managers' eager init now adopts only the
      // local cache, so prepare() never sends a request.

      final ready = AccountSnapshot(
        phase: AccountPhase.ready,
        context: context,
        isEmailVerified: credential.isVerified,
      );
      _publish(ready);
      return TransitionReady(context);
    } on Object catch (error, stackTrace) {
      await _popScope();
      _current = null;

      // A session/token missing during prepare() is the expected boot path when
      // something still reached the network: publish ready rather than block —
      // Step 13 says a bare error must not do a blanket sign-out.
      if (error is MenoException &&
          error.code == MenoErrorCode.sessionUnavailable) {
        _logger.w(
          'A request was refused during prepare(); publishing ready with the '
          'credential on disk and letting the subtree refresh.',
          tag: MenoLogTag.di.label,
        );
        final ready = AccountSnapshot(
          phase: AccountPhase.ready,
          context: context,
          isEmailVerified: credential.isVerified,
        );
        _publish(ready);
        return TransitionReady(context);
      }

      const failure = BlockedFailure(
        reason: BlockedReason.registrationFailed,
        message: 'The account could not be prepared.',
      );
      _publish(
        const AccountSnapshot(phase: AccountPhase.blocked, failure: failure),
      );
      _logger.w(
        'Scoping the account failed',
        tag: MenoLogTag.di.label,
        error: error,
        stackTrace: stackTrace,
      );
      return const TransitionBlocked(failure);
    }
  }

  Future<TransitionResult> _exit({CredentialRecord? next}) async {
    if (_disposed) return const TransitionCancelled();

    if (_hasScope || di.hasScope(Scope.authenticated)) {
      final context = _current == null
          ? null
          : AccountContext(
              accountId: _current!.accountId,
              generation: _generation,
            );
      _publish(
        AccountSnapshot(
          phase: AccountPhase.quiescing,
          context: context,
          isEmailVerified: _current?.isVerified ?? false,
        ),
      );

      // The gate replaces the subtree with a neutral page in that build, so the
      // next frame is the signal that the old subtree is gone.
      await _endOfFrame();

      try {
        await beginLiveExit?.call();
        await _popScope();
      } on Object catch (error, stackTrace) {
        const failure = BlockedFailure(
          reason: BlockedReason.cleanupIncomplete,
          message: 'The previous account could not be closed.',
        );
        _publish(
          const AccountSnapshot(phase: AccountPhase.blocked, failure: failure),
        );
        _logger.e(
          'Closing the account scope failed',
          tag: MenoLogTag.di.label,
          error: error,
          stackTrace: stackTrace,
        );
        return const TransitionBlocked(failure);
      }
    }

    _current = null;
    _publish(AccountSnapshot.signedOut);

    if (next != null && _desiredAccountId == next.accountId) {
      return await _prepare(next);
    }
    return const TransitionCancelled();
  }

  Future<void> _popScope() async {
    if (di.hasScope(Scope.authenticated)) {
      await di.popScopesTill(Scope.root, inclusive: false);
    }
    _hasScope = false;
  }

  /// Waits for one frame, bounded so a transition cannot hang forever when no
  /// frame is scheduled (a background isolate or a unit test).
  Future<void> _endOfFrame() async {
    try {
      await SchedulerBinding.instance.endOfFrame.timeout(frameWait);
    } on TimeoutException {
      // No frame is coming; the caller's own scope pop still releases
      // resources.
    }
  }

  void _publish(AccountSnapshot next) {
    if (_disposed) return;
    if (snapshot.value == next) return;
    snapshot.value = next;
  }

  Id? _accountIdOf(CredentialRecord? record) => record?.accountId;
}
