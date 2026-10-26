/// What credential one request travels with.
///
/// It is decided by the caller and never inferred from the path: a path string
/// is not a statement about identity, and a new endpoint that forgets to be
/// listed would otherwise silently receive an account's token (or fail to).
sealed class AuthMode {
  const new();

  /// The key a request carries its [AuthMode] under, so the auth policy can
  /// read the decision without guessing.
  static const String extraKey = 'meno.auth.mode';

  /// Nobody: no `Authorization` header, even when somebody is signed in.
  static const AuthMode anonymous = AnonymousAuth();

  /// The account active when the request is dispatched, pinned to that
  /// identity for its whole life.
  static const AuthMode account = AccountAuth();
}

/// Send no credential at all.
final class AnonymousAuth extends AuthMode {
  const new();
}

/// Send the active account's access token.
final class AccountAuth extends AuthMode {
  const new();
}

/// Send exactly [token], once, with no renewal and no replay.
///
/// Used by logout: the token was captured before the record was removed, so
/// there is nothing left to look up and nothing to renew.
final class BearerAuth extends AuthMode {
  const new(this.token);

  final String token;
}
