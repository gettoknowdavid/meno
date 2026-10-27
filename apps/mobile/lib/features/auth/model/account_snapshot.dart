import 'package:flutter/foundation.dart';
import 'package:meno_mobile/features/auth/model/auth_context.dart';

/// Where the app is in the account lifecycle.
enum AccountPhase {
  /// Nobody is signed in; only public routes may build.
  signedOut,

  /// An account is being scoped and is not usable yet.
  preparing,

  /// The account's scope exists and the account is usable.
  ready,

  /// The account is being taken down; the gate shows a neutral page.
  quiescing,

  /// The transition failed and cannot continue without the user.
  blocked,
}

/// Why an account transition stopped.
enum BlockedReason {
  /// There is no credential that can be used without signing in again.
  authRequired,

  /// Registering the account's services failed.
  registrationFailed,

  /// Tearing the previous account down failed.
  cleanupIncomplete,
}

/// A typed reason for [AccountPhase.blocked].
@immutable
final class BlockedFailure {
  const new({required this.reason, required this.message});

  final BlockedReason reason;
  final String message;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is BlockedFailure &&
          runtimeType == other.runtimeType &&
          reason == other.reason &&
          message == other.message;

  @override
  int get hashCode => Object.hash(reason, message);

  @override
  String toString() => 'BlockedFailure(${reason.name}, $message)';
}

/// The single readiness observable for the account lifecycle.
///
/// Equality matters: a same-value re-publish must not rebuild a watcher, and a
/// generation change must.
@immutable
final class AccountSnapshot {
  const new({
    required this.phase,
    this.context,
    this.isEmailVerified = false,
    this.failure,
  });

  /// Nobody is signed in.
  static const signedOut = AccountSnapshot(phase: AccountPhase.signedOut);

  /// An account is being prepared.
  static const preparing = AccountSnapshot(phase: AccountPhase.preparing);

  /// The previous account is being taken down.
  static const quiescing = AccountSnapshot(phase: AccountPhase.quiescing);

  final AccountPhase phase;

  /// Present only for [AccountPhase.ready] and [AccountPhase.quiescing].
  final AccountContext? context;

  final bool isEmailVerified;

  /// Present only for [AccountPhase.blocked].
  final BlockedFailure? failure;

  bool get isReady => phase == AccountPhase.ready;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is AccountSnapshot &&
          runtimeType == other.runtimeType &&
          phase == other.phase &&
          context == other.context &&
          isEmailVerified == other.isEmailVerified &&
          failure == other.failure;

  @override
  int get hashCode =>
      Object.hash(phase, context, isEmailVerified, failure);

  @override
  String toString() =>
      'AccountSnapshot(${phase.name}, context: $context, '
      'verified: $isEmailVerified, failure: $failure)';
}
