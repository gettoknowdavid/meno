import 'package:flutter/foundation.dart';

/// Owns the authentication session state for the app.
///
/// State is exposed as [ValueNotifier]s so the router guard and reactive
/// widgets (via `watchValue`) all observe the same source of truth. This is a
/// thin placeholder until the auth API/services are implemented.
final class AuthManager {
  /// Whether a user is currently signed in.
  final ValueNotifier<bool> isAuthenticated = ValueNotifier<bool>(false);

  /// Whether the signed-in user's email has been verified.
  final ValueNotifier<bool> emailVerified = ValueNotifier<bool>(false);

  /// Marks the session as signed in.
  void signIn({bool verified = false}) {
    emailVerified.value = verified;
    isAuthenticated.value = true;
  }

  /// Clears the current session.
  void signOut() {
    isAuthenticated.value = false;
    emailVerified.value = false;
  }

  /// Marks the current user's email as verified.
  void verifyEmail() {
    emailVerified.value = true;
  }
}
