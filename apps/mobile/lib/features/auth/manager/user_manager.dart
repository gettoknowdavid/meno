import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/manager/auth_manager.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';

/// The read-only view of who is signed in.
///
/// Everything else in the app needs the current user and nothing more. Handing
/// them [AuthManager] would also hand them the ability to sign the account out
/// or switch it, which is a decision only the auth screens should make — so the
/// dependency features take is this one, and it forwards state only.
///
/// It lives in the auth feature rather than in `_shared` because it is defined
/// in terms of what auth owns. `_shared` stays the bottom layer: services,
/// models and layouts that depend on no feature.
final class UserManager {
  /// Creates a view over [_auth].
  const new(this._auth);

  final AuthManager _auth;

  /// The account every request is made as.
  ValueListenable<UserCredential> get currentCredential =>
      _auth.activeCredential;

  /// The signed-in user, or [User.empty].
  ValueListenable<User> get currentUser =>
      _auth.activeCredential.select((credential) => credential.user);

  /// The id of the signed-in account, or null.
  ValueListenable<String?> get currentUserId =>
      _auth.activeCredential.select((credential) => credential.userId);

  /// Whether an account is signed in.
  ValueListenable<bool> get isAuthenticated => _auth.isAuthenticated;

  /// Whether the signed-in account's address has been confirmed.
  ValueListenable<bool> get isEmailVerified => _auth.emailVerified;

  /// The account that last signed in on this device, kept across a sign-out.
  ValueListenable<User> get lastKnownUser => _auth.lastKnownUser;

  /// Every account signed in on this device, keyed by user id.
  ValueListenable<Map<String, UserCredential>> get accounts => _auth.accounts;
}
