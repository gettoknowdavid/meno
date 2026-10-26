import 'package:meno_mobile/features/auth/model/credential_record.dart';

/// What a token renewal produced.
///
/// Four cases, because only four things can be known for sure: the pair was
/// replaced, the server refused, the attempt provably never reached the server,
/// or the outcome cannot be known (a lost response). The last one must never be
/// retried — refresh tokens are single use.
sealed class RefreshOutcome {
  const new();
}

final class RefreshSucceeded extends RefreshOutcome {
  const new(this.record);

  /// The freshly issued record for the same account and epoch.
  final CredentialRecord record;
}

/// The server documented the session as invalid, expired or revoked.
final class RefreshRejected extends RefreshOutcome {
  const new();
}

/// The attempt did not rotate the token (offline, connection failure, or a
/// documented 429/maintenance response).
final class RefreshUnavailable extends RefreshOutcome {
  const new({this.retryAfter});

  /// How long the server asked the client to wait, when it said so.
  final Duration? retryAfter;
}

/// The token may already have been spent. Requires reauthentication, never an
/// automatic retry.
final class RefreshUncertain extends RefreshOutcome {
  const new(this.reason);

  final String reason;
}
