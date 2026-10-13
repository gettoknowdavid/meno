/// Names of the values the app stores on the device.
///
/// Secret values live in secure storage and everything else in shared
/// preferences; both are addressed by name, so the names live in one place
/// instead of inside the features that read them. A key is a persistence
/// contract: renaming one orphans whatever is already on a device.
final class StorageKeys {
  /// Prevents instantiation: this type is a namespace for the keys.
  const new _();

  // Secure storage — the credential the API hands out, split into the fields
  // the app needs individually so the auth interceptor can read a token without
  // decoding the whole credential.
  static const String accessToken = 'current_user_token';
  static const String refreshToken = 'current_refresh_token';
  static const String sessionExpiry = 'session_expiry';
  static const String credential = 'current_user_credential';
  static const String accounts = 'all_accounts';
  static const String userId = 'current_user_id';
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

  /// [key] scoped to [userId].
  ///
  /// Device storage is shared by every account that signs in on it, so anything
  /// that belongs to a user has to be scoped to them.
  static String forUser(String key, String userId) => '${key}_$userId';
}
