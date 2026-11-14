/// What a token renewal produced.
///
/// Four cases, because only four things can be known for sure: the pair was
/// replaced, the server refused, the attempt provably never reached the server,
/// or the outcome cannot be known (a lost response). The last one must never be
/// retried — refresh tokens are single use.
///
/// The payload is generic: the vault's settlement path holds a
/// `CredentialRecord`, the live interceptor path holds a `UserCredential`. The
/// four outcomes are the same classification either way.
sealed class RefreshOutcome<T> {
  const new();
}

final class RefreshSucceeded<T> extends RefreshOutcome<T> {
  const new(this.credential);

  /// The freshly issued credential for the same account.
  final T credential;
}

/// The server documented the session as invalid, expired or revoked.
final class RefreshRejected<T> extends RefreshOutcome<T> {
  const new();
}

/// The attempt did not rotate the token (offline, connection failure, or a
/// documented 429/maintenance response). The existing pair is still the truth
/// and may keep being used while the access token is valid.
final class RefreshUnavailable<T> extends RefreshOutcome<T> {
  const new({this.retryAfter});

  /// How long the server asked the client to wait, when it said so.
  final Duration? retryAfter;
}

/// The token may already have been spent. Requires reauthentication, never an
/// automatic retry.
final class RefreshUncertain<T> extends RefreshOutcome<T> {
  const new(this.reason);

  final String reason;
}
