//
// ignore_for_file: avoid_equals_and_hash_code_on_mutable_classes

/// The tokens one signed-in account holds on this device.
///
/// The expiry is a real `DateTime`, not a decoded JWT: the API sends
/// `expires_in` (seconds until the access token stops being valid) with every
/// token pair, so there is nothing to decode and no clock skew to guess at.
final class Session {
  /// Creates a session.
  const new({required this.accessToken, this.refreshToken, this.expiresAt});

  /// The bearer token to send on requests.
  final String accessToken;

  /// The single-use token that buys a new pair.
  ///
  /// The API rotates refresh tokens, so this is replaced on every refresh —
  /// presenting the same one twice ends the session.
  final String? refreshToken;

  /// When [accessToken] stops being accepted.
  final DateTime? expiresAt;

  /// A session with no tokens: nobody is signed in.
  static const Session empty = Session(accessToken: '');

  /// Whether the access token is past its expiry.
  bool get isExpired {
    final expiry = expiresAt;
    return expiry != null && !DateTime.now().isBefore(expiry);
  }

  /// Whether this session can be renewed without the user signing in again.
  bool get isRefreshable {
    final token = refreshToken;
    return token != null && token.isNotEmpty;
  }

  /// Whether the access token is past, or about to pass, its expiry.
  ///
  /// Refreshing before the token dies rather than after: a request that goes
  /// out on a token with two seconds left is a request that fails, and the
  /// failure lands on whatever screen happened to be open.
  bool shouldRefresh({Duration threshold = const Duration(minutes: 1)}) {
    if (!isRefreshable) return false;
    final expiry = expiresAt;
    if (expiry == null) return false;
    return !DateTime.now().add(threshold).isBefore(expiry);
  }

  /// How long the access token has left, or null when the expiry is unknown.
  Duration? get timeUntilExpiry => expiresAt?.difference(DateTime.now());

  @override
  bool operator ==(Object other) =>
      other is Session &&
      other.accessToken == accessToken &&
      other.refreshToken == refreshToken &&
      other.expiresAt == expiresAt;

  @override
  int get hashCode => Object.hash(accessToken, refreshToken, expiresAt);
}
