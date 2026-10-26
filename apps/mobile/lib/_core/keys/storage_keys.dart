/// Names of the values the app stores on the device.
///
/// Secret values live in secure storage and everything else in shared
/// preferences; both are addressed by name, so the names live in one place
/// instead of inside the features that read them. A key is a persistence
/// contract: renaming one orphans whatever is already on a device.
final class StorageKeys {
  /// Prevents instantiation: this type is a namespace for the keys.
  const new _();

  /// The vault for storing sensitive authentication data.
  static const String vault = 'auth.vault';

  @Deprecated('Use [vault] instead.')
  static const String accessToken = 'current_user_token';

  @Deprecated('Use [vault] instead.')
  static const String refreshToken = 'current_refresh_token';

  @Deprecated('Use [vault] instead.')
  static const String sessionExpiry = 'session_expiry';

  @Deprecated('Use [vault] instead.')
  static const String credential = 'current_user_credential';

  @Deprecated('Use [vault] instead.')
  static const String accounts = 'all_accounts';

  @Deprecated('Use [vault] instead.')
  static const String userId = 'current_user_id';

  /// The FCM token for the device, used for push notifications.
  static const String fcmToken = 'fcm_token';

  // Shared preferences.
  static const String onboardingCompleted = 'onboarding.completed';

  /// The signed-in account's own profile, as it was last fetched.
  ///
  /// Scoped to the account it describes through [forUser], so switching
  /// accounts shows that account's profile rather than the previous one's.
  static const String profile = 'profile.me';

  /// The signed-in account's settings, as they were last fetched.
  ///
  /// Scoped to the account it describes through [forUser], so switching
  /// accounts shows that account's switches rather than the previous one's.
  static const String settings = 'settings.me';

  /// A profile that has been opened, kept so returning to it is instant.
  static const String publicProfile = 'profile.public';

  /// The live broadcast this account was in when the app stopped.
  ///
  /// Scoped to the account it belongs to through [forUser]: a session is
  /// entered with a token minted for one account, and re-entering it as another
  /// would join the room under the wrong identity. Written when a session opens
  /// and cleared when it ends, so what is here is only ever a session that was
  /// still running the last time the process stopped.
  static const String liveSession = 'broadcast.live_session';

  /// [key] scoped to [userId].
  ///
  /// Device storage is shared by every account that signs in on it, so anything
  /// that belongs to a user has to be scoped to them.
  static String forUser(String key, String userId) => '${key}_$userId';

  static String permissionPrimerDeclined(String typeName) {
    return 'permission-primer-$typeName';
  }
}
