import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/auth/manager/auth_manager.dart';
import 'package:meno_mobile/features/auth/model/entities/user_credential.dart';

/// Keeps a get_it scope in step with the signed-in account.
///
/// ## Why a scope, and not just a registration
///
/// A scope is a registration *layer* with a lifetime. Everything registered in
/// [Scope.authenticated] is created when an account becomes active and disposed
/// when it stops being — together, in one call, with no list of services to
/// remember to tear down. That is the shape the app needs for per-account state:
/// one account's notes, drafts, sockets and caches must not survive into the
/// next account's session, and a scope is the mechanism that guarantees it.
///
/// ## What it does here
///
/// It follows [AuthManager.activeCredential] — which already folds together
/// signing in, switching accounts, renewing a token and being signed out — and
/// pushes or pops the scope to match:
///
/// * a different account becomes active → the old scope is popped (disposing
///   everything registered in it) and a new one is pushed for the new account;
/// * the same account's tokens are renewed → nothing moves, so a network
///   refresh never tears down the user's screens;
/// * nobody is signed in → the scope is gone.
///
/// ## Adding to the scope
///
/// [_registerDependencies] is the extension point. A feature that owns
/// per-account objects adds its registration there, and inherits the lifetime
/// handling for free.
final class UserScopeManager implements Disposable {
  /// Creates a manager that follows [_auth].
  UserScopeManager(this._auth);

  final AuthManager _auth;

  ListenableSubscription? _subscription;
  String? _scopedUserId;

  /// Subscribes to the active account, then applies the one already in effect.
  ///
  /// Registered with `dependsOn: [AuthManager]`, so the session restored at
  /// startup is in place before this runs and the first scope is pushed for the
  /// right account.
  Future<UserScopeManager> init() async {
    _subscription = _auth.activeCredential.listen(_onActiveCredentialChanged);
    // Deferred by a microtask rather than awaited: this runs while the locator is
    // still starting the container, and pushing a scope from inside another
    // registration's initializer would change the scope stack under it. By the
    // time the app's startup gate finishes there is a scope in place either way.
    scheduleMicrotask(() {
      unawaited(_apply(_auth.activeCredential.value));
    });
    return this;
  }

  /// Releases the subscription.
  @override
  FutureOr<dynamic> onDispose() {
    _subscription?.cancel();
  }

  /// Follows the active account.
  void _onActiveCredentialChanged(
    UserCredential credential,
    ListenableSubscription subscription,
  ) {
    unawaited(_apply(credential));
  }

  /// Pushes the scope for [credential], or pops it when nobody is signed in.
  Future<void> _apply(UserCredential credential) async {
    if (credential.userId == _scopedUserId) return;

    await _popScope();
    if (credential.isEmpty) return;

    await di.pushNewScopeAsync(
      scopeName: Scope.authenticated,
      init: (getIt) async => _registerDependencies(getIt, credential),
    );
    _scopedUserId = credential.userId;
  }

  /// Registers everything that belongs to one account.
  ///
  /// The active credential is the minimum: a screen that needs the signed-in
  /// user's tokens reads them from here rather than reaching into storage.
  void _registerDependencies(GetIt getIt, UserCredential credential) {
    getIt.registerSingleton<UserCredential>(credential);
  }

  /// Pops the account scope, if it is the one on top.
  Future<void> _popScope() async {
    if (di.currentScopeName != Scope.authenticated) {
      _scopedUserId = null;
      return;
    }

    await di.popScopesTill(Scope.root, inclusive: false);
    _scopedUserId = null;
  }
}
