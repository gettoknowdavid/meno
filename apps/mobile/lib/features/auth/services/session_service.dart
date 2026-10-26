import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/exceptions/meno_exception.dart';
import 'package:meno_mobile/_core/keys/storage_keys.dart';
import 'package:meno_mobile/_core/value_objects/id.dart';
import 'package:meno_mobile/_shared/services/secure_storage.dart';
import 'package:meno_mobile/features/auth/model/credential_record.dart';
import 'package:meno_mobile/features/auth/model/mutation_result.dart';
import 'package:meno_mobile/features/auth/model/refresh_outcome.dart';

/// Where the accounts signed in on this device live.
///
/// One versioned document under [StorageKeys.vaultV2] holds every account's
/// credential, the active account id, and the remembered greeting identities.
/// There is exactly one writer and one in-memory copy of the truth.
///
/// Every mutation goes through one FIFO queue, so a renewal, a switch and a
/// logout cannot interleave their read-modify-write cycles. Inside that queue
/// the expected account, epoch and revision are compared against the committed
/// envelope; a mutation whose expectation is no longer true returns
/// [MutationStale] and writes nothing.
///
/// Storage is written first and memory is updated only after the write
/// succeeded. A failed write therefore leaves memory exactly as it was, which
/// is what makes a single "could not save, retry" state enough — there is no
/// global gate and no recovery protocol.
///
/// The queue never waits on the network or on the account transition; it only
/// waits on the secure store. That keeps a slow logout from blocking a token
/// renewal and vice versa.
final class SessionService implements Disposable {
  new(this._storage);

  final SecureStorage _storage;

  final ValueNotifier<VaultEnvelope?> _changes = ValueNotifier(null);

  Future<void> _queue = Future.value();
  Future<void>? _initializing;
  Future<void>? _closed;
  VaultEnvelope? _envelope;
  bool _isClosing = false;
  bool _projectionsClosed = false;

  /// The committed envelope, published once per successful mutation.
  ///
  /// Null until [initialize] has completed. A listener can rebuild on it the
  /// same way it would on any other value owned by a manager.
  ValueListenable<VaultEnvelope?> get changes => _changes;

  bool get isInitialized => _envelope != null;

  bool get isClosing => _isClosing;

  /// Reads the stored vault into memory.
  ///
  /// An absent, unreadable or invalid document is treated as signed out: the
  /// service starts from an empty envelope rather than from a partial decode.
  /// A storage read failure also starts signed out; nothing is deleted, so the
  /// next launch can still recover the document.
  Future<void> initialize() {
    if (_envelope != null) return Future.value();
    return _initializing ??= _load();
  }

  /// The committed envelope. Synchronous after [initialize].
  ///
  /// Throws [StateError] when called before initialisation, because every
  /// caller of this method is account-scoped code that must wait for readiness.
  VaultEnvelope readSnapshot() {
    final envelope = _envelope;
    if (envelope == null) {
      throw StateError(
        'SessionService.initialize() must complete before readSnapshot().',
      );
    }
    return envelope;
  }

  /// The credential a request may use for [accountId].
  ///
  /// Null when the account is unknown, when a renewal outcome is uncertain
  /// (that token pair must never be spent again), or when the access token has
  /// expired and there is no refresh token left to recover it. An expired
  /// access token that still has a refresh token is returned: the interceptor
  /// needs the record in order to renew it.
  CredentialRecord? usableCredential(Id accountId) {
    final record = _envelope?.credentials[accountId];
    if (record == null) return null;
    if (record.renewalState == RenewalState.uncertain) return null;

    final stillValid = record.accessExpiresAt.isAfter(DateTime.now().toUtc());
    if (stillValid) return record;

    final refreshToken = record.refreshToken;
    if (refreshToken == null || refreshToken.isEmpty) return null;
    return record;
  }

  /// Files a freshly issued credential.
  ///
  /// A record for the same account and epoch is already committed, so adopting
  /// it again changes nothing; a new epoch (a fresh login or verification)
  /// replaces the stored record and moves the account to the front of the
  /// last-used order.
  Future<MutationResult> adoptCandidate(CredentialRecord record) {
    return _enqueue(() {
      final guard = _guard();
      if (guard != null) return guard;
      if (!record.accountId.isValid) {
        return const MutationRejected(MutationRejection.invalidEnvelope);
      }

      final current = _envelope!;
      final existing = current.credentials[record.accountId];
      if (existing != null && existing.epoch == record.epoch) {
        return MutationCommitted(current.revision);
      }
      return _commit(current, _putCredential(current, record, touch: true));
    });
  }

  /// Makes [accountId] the active account.
  ///
  /// The transition owner calls this once the account's scope is ready; the
  /// vault only records the choice, it does not decide it.
  Future<MutationResult> activate(Id accountId) {
    return _enqueue(() {
      final guard = _guard();
      if (guard != null) return guard;

      final current = _envelope!;
      if (!current.credentials.containsKey(accountId)) {
        return const MutationRejected(MutationRejection.unknownAccount);
      }
      if (current.activeAccountId == accountId) {
        return MutationCommitted(current.revision);
      }
      return _commit(
        current,
        current.copyWith(
          activeAccountId: accountId,
          orderedAccountIds: _touchOrder(current, accountId),
        ),
      );
    });
  }

  /// Clears the active account while keeping every credential and remembered
  /// identity.
  ///
  /// This is a switch or an add-account, not a logout; it must never be logged
  /// or messaged as one.
  Future<MutationResult> deactivate() {
    return _enqueue(() {
      final guard = _guard();
      if (guard != null) return guard;

      final current = _envelope!;
      if (current.activeAccountId == null) {
        return MutationCommitted(current.revision);
      }
      return _commit(current, current.copyWith(activeAccountId: null));
    });
  }

  /// Replaces the token pair only when the caller's expectation still holds.
  ///
  /// [fromRevision] is the revision the caller saw before it started the
  /// renewal. If another mutation already moved the record on, the result is
  /// [MutationStale] and the newer tokens are not overwritten.
  ///
  /// Omit [refreshToken] to keep the stored one; the backend may return an
  /// access-only pair.
  Future<MutationResult> conditionalReplace(
    Id accountId, {
    required String epoch,
    required int fromRevision,
    required String accessToken,
    required DateTime accessExpiresAt,
    String? refreshToken,
  }) {
    return _conditional(
      (current) => _matches(current, accountId, epoch, fromRevision),
      (current) {
        final existing = current.credentials[accountId]!;
        return _putCredential(
          current,
          existing.copyWith(
            accessToken: accessToken,
            refreshToken: refreshToken ?? existing.refreshToken,
            accessExpiresAt: accessExpiresAt.toUtc(),
            revision: existing.revision + 1,
            renewalState: RenewalState.idle,
          ),
        );
      },
    );
  }

  /// Persists that a renewal is about to be dispatched.
  ///
  /// Written before the request leaves so a crash in the middle is recovered as
  /// [RenewalState.uncertain] on the next start, which requires a re-login
  /// instead of a second use of a possibly spent refresh token.
  Future<MutationResult> markRenewalInFlight(
    Id accountId, {
    required String epoch,
    required int revision,
  }) {
    return _enqueue(() {
      final guard = _guard();
      if (guard != null) return guard;

      final current = _envelope!;
      if (!_matches(current, accountId, epoch, revision)) {
        return const MutationStale();
      }
      final existing = current.credentials[accountId]!;
      if (existing.renewalState == RenewalState.inFlight) {
        return MutationCommitted(current.revision);
      }
      return _commit(
        current,
        _putCredential(
          current,
          existing.copyWith(renewalState: RenewalState.inFlight),
        ),
      );
    });
  }

  /// Applies a renewal outcome to the record the renewal started from.
  ///
  /// Requires the same account, epoch and revision that
  /// [markRenewalInFlight] recorded, so a late outcome can never settle a
  /// newer session.
  Future<MutationResult> settleRenewal(
    Id accountId, {
    required String epoch,
    required int revision,
    required RefreshOutcome<CredentialRecord> outcome,
  }) {
    return _enqueue(() {
      final guard = _guard();
      if (guard != null) return guard;

      final current = _envelope!;
      if (!_matches(current, accountId, epoch, revision)) {
        return const MutationStale();
      }
      final existing = current.credentials[accountId]!;

      switch (outcome) {
        case RefreshSucceeded(:final credential):
          if (credential.accountId != accountId) return const MutationStale();
          return _commit(
            current,
            _putCredential(
              current,
              existing.copyWith(
                accessToken: credential.accessToken,
                refreshToken: credential.refreshToken,
                accessExpiresAt: credential.accessExpiresAt.toUtc(),
                revision: existing.revision + 1,
                renewalState: RenewalState.idle,
              ),
            ),
          );
        case RefreshRejected():
          return _commit(current, _withoutCredential(current, accountId));
        case RefreshUnavailable():
          return _commit(
            current,
            _putCredential(
              current,
              existing.copyWith(renewalState: RenewalState.idle),
            ),
          );
        case RefreshUncertain():
          return _commit(
            current,
            _putCredential(
              current,
              existing.copyWith(renewalState: RenewalState.uncertain),
            ),
          );
      }
    });
  }

  /// Explicit logout: deletes the account's usable credentials.
  ///
  /// The remembered identity survives so the signed-out screen can greet the
  /// user, but it is flagged as requiring authentication. A mismatch between
  /// the stored epoch and [epoch] means a newer login already replaced the
  /// record, so nothing is deleted.
  Future<MutationResult> removeCredentials(
    Id accountId, {
    required String epoch,
  }) {
    return _enqueue(() {
      final guard = _guard();
      if (guard != null) return guard;

      final current = _envelope!;
      final existing = current.credentials[accountId];
      if (existing == null) return MutationCommitted(current.revision);
      if (existing.epoch != epoch) return const MutationStale();
      return _commit(current, _withoutCredential(current, accountId));
    });
  }

  /// Updates greeting metadata. Never accepts or writes a token.
  Future<MutationResult> updateRememberedIdentity(RememberedIdentity identity) {
    return _enqueue(() {
      final guard = _guard();
      if (guard != null) return guard;
      if (!identity.accountId.isValid) {
        return const MutationRejected(MutationRejection.invalidEnvelope);
      }

      final current = _envelope!;
      return _commit(
        current,
        current.copyWith(
          remembered: {...current.remembered, identity.accountId: identity},
        ),
      );
    });
  }

  /// Stops accepting mutations and waits for the ones already queued.
  ///
  /// Idempotent. Callers dispose the service after this future completes.
  Future<void> close() => _closed ??= _close();

  @override
  FutureOr<dynamic> onDispose() => close();

  Future<void> _load() async {
    VaultEnvelope? stored;
    try {
      final raw = await _storage.read(StorageKeys.vaultV2);
      stored = _decode(raw);
    } on StorageException {
      stored = null;
    }
    if (_isClosing) return;

    _envelope = _recoverInterruptedRenewals(stored ?? _emptyEnvelope());
    _emit(_envelope!);
  }

  Future<void> _close() async {
    _isClosing = true;
    await _queue;
    if (_projectionsClosed) return;
    _projectionsClosed = true;
    _changes.dispose();
  }

  /// Serialises [job] behind everything already queued.
  ///
  /// The returned future carries the job's result (or error); the queue tail
  /// itself always completes, so one rejected job cannot poison the next one.
  Future<T> _enqueue<T>(FutureOr<T> Function() job) {
    final completer = Completer<T>();
    _queue = _queue.then((_) async {
      try {
        completer.complete(await job());
      } on Object catch (error, stackTrace) {
        completer.completeError(error, stackTrace);
      }
    });
    return completer.future;
  }

  /// Runs [apply] only when [matches] is still true inside the queue.
  Future<MutationResult> _conditional(
    bool Function(VaultEnvelope current) matches,
    VaultEnvelope Function(VaultEnvelope current) apply,
  ) {
    return _enqueue(() {
      final guard = _guard();
      if (guard != null) return guard;

      final current = _envelope!;
      if (!matches(current)) return const MutationStale();
      return _commit(current, apply(current));
    });
  }

  Future<MutationResult> _commit(
    VaultEnvelope current,
    VaultEnvelope proposed,
  ) async {
    final next = proposed.copyWith(
      schemaVersion: VaultEnvelope.schemaVersionNumber,
      revision: current.revision + 1,
    );
    if (!_isValid(next)) {
      return const MutationRejected(MutationRejection.invalidEnvelope);
    }

    try {
      await _storage.write(
        StorageKeys.vaultV2,
        value: jsonEncode(next.toJson()),
      );
    } on StorageException {
      return const MutationStorageFailed();
    }

    _envelope = next;
    _emit(next);
    return MutationCommitted(next.revision);
  }

  MutationResult? _guard() {
    if (_isClosing) {
      return const MutationRejected(MutationRejection.closing);
    }
    if (_envelope == null) {
      return const MutationRejected(MutationRejection.notInitialized);
    }
    return null;
  }

  /// A renewal that was in flight when the last process stopped may already
  /// have spent its refresh token, so it is treated as uncertain from now on.
  VaultEnvelope _recoverInterruptedRenewals(VaultEnvelope envelope) {
    final interrupted = envelope.credentials.values
        .where((record) => record.renewalState == RenewalState.inFlight)
        .toList();
    if (interrupted.isEmpty) return envelope;

    final credentials = {...envelope.credentials};
    for (final record in interrupted) {
      credentials[record.accountId] = record.copyWith(
        renewalState: RenewalState.uncertain,
      );
    }
    return envelope.copyWith(credentials: credentials);
  }

  bool _matches(
    VaultEnvelope current,
    Id accountId,
    String epoch,
    int revision,
  ) {
    final record = current.credentials[accountId];
    return record != null &&
        record.epoch == epoch &&
        record.revision == revision;
  }

  VaultEnvelope _putCredential(
    VaultEnvelope current,
    CredentialRecord record, {
    bool touch = false,
  }) {
    final credentials = {...current.credentials, record.accountId: record};
    if (!touch) return current.copyWith(credentials: credentials);
    return current.copyWith(
      credentials: credentials,
      orderedAccountIds: _touchOrder(current, record.accountId),
    );
  }

  VaultEnvelope _withoutCredential(VaultEnvelope current, Id accountId) {
    final credentials = {...current.credentials}..remove(accountId);
    final ordered = [
      for (final id in current.orderedAccountIds)
        if (id != accountId) id,
    ];

    final remembered = {...current.remembered};
    final identity = remembered[accountId];
    if (identity != null && !identity.requiresAuthentication) {
      remembered[accountId] = identity.copyWith(requiresAuthentication: true);
    }

    return current.copyWith(
      credentials: credentials,
      orderedAccountIds: ordered,
      remembered: remembered,
      activeAccountId: current.activeAccountId == accountId
          ? null
          : current.activeAccountId,
    );
  }

  List<Id> _touchOrder(VaultEnvelope current, Id accountId) => [
    for (final id in current.orderedAccountIds)
      if (id != accountId) id,
    accountId,
  ];

  bool _isValid(VaultEnvelope envelope) {
    if (envelope.schemaVersion != VaultEnvelope.schemaVersionNumber) {
      return false;
    }
    if (envelope.revision < 0) return false;

    for (final entry in envelope.credentials.entries) {
      if (!entry.key.isValid || entry.value.accountId != entry.key) {
        return false;
      }
    }

    final ids = envelope.credentials.keys.toSet();
    if (envelope.orderedAccountIds.length != ids.length) return false;
    if (envelope.orderedAccountIds.toSet().length != ids.length) return false;
    if (!ids.containsAll(envelope.orderedAccountIds)) return false;

    final active = envelope.activeAccountId;
    if (active != null && !ids.contains(active)) return false;

    for (final entry in envelope.remembered.entries) {
      if (!entry.key.isValid || entry.value.accountId != entry.key) {
        return false;
      }
    }
    return true;
  }

  VaultEnvelope? _decode(String? raw) {
    if (raw == null || raw.isEmpty) return null;
    try {
      final decoded = jsonDecode(raw);
      final envelope = VaultEnvelope.tryFromJson(decoded);
      if (envelope == null || !_isValid(envelope)) return null;
      return envelope;
    } on FormatException {
      return null;
    }
  }

  void _emit(VaultEnvelope envelope) {
    if (_projectionsClosed) return;
    _changes.value = envelope;
  }
}

/// The signed-out vault: nothing stored, nothing active.
VaultEnvelope _emptyEnvelope() => VaultEnvelope(
  schemaVersion: VaultEnvelope.schemaVersionNumber,
  revision: 0,
  orderedAccountIds: const [],
  credentials: const {},
  remembered: const {},
);
