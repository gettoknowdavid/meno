import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/auth/manager/auth_manager.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';

/// Keeps a get_it scope in step with the signed-in account.
///
/// A scope is a registration *layer* with a lifetime: everything registered in
/// [Scope.authenticated] is created when an account becomes active and disposed
/// when it stops being, so one account's notes, drafts, sockets and caches
/// cannot outlive their account or leak into the next. This is the mechanism
/// that guarantees it.
///
/// It follows [AuthManager.activeCredential] — which already folds together
/// signing in, switching accounts, renewing a token and being signed out — and
/// pushes or pops the scope to match:
///
/// * a different account becomes active → the old scope is popped and a new one
///   pushed for the new account;
/// * the same account's tokens are renewed → nothing moves, so a network
///   refresh never tears down the user's screens;
/// * nobody is signed in → the scope is gone.
final class UserScopeManager implements Disposable {
  /// Creates a manager that follows [_auth].
  new(this._auth);

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
    // Deferred by a microtask rather than awaited: this runs while the locator
    // is still starting the container, and pushing a scope from inside another
    // registration's initializer would change the scope stack under it. By the
    // time the app's startup gate finishes there is a scope in place
    // either way.
    scheduleMicrotask(() {
      unawaited(_apply(_auth.activeCredential.value));
    });
    return this;
  }

  /// Pushes the scope for [credential], or pops it when nobody is signed in.
  Future<void> _apply(UserCredential credential) async {
    if (credential.userId == _scopedUserId) return;

    await _popScope();
    if (credential.isEmpty) return;

    await di.pushNewScopeAsync(
      scopeName: Scope.authenticated,
      init: (getIt) async {
        getIt.registerSingleton<UserCredential>(credential);
      },
    );
    _scopedUserId = credential.userId;
  }

  /// Follows the active account.
  void _onActiveCredentialChanged(
    UserCredential credential,
    ListenableSubscription subscription,
  ) {
    unawaited(_apply(credential));
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

  /// Releases the subscription.
  @override
  FutureOr<dynamic> onDispose() {
    _subscription?.cancel();
  }
}
