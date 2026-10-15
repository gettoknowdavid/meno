import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/auth/manager/auth_manager.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';

/// Registers the services that belong to one account, in that account's scope.
typedef AccountScopeRegistrar = FutureOr<void> Function(
  GetIt scope,
  UserCredential credential,
);

/// Keeps a get_it scope in step with the signed-in account.
///
/// A scope is a registration *layer* with a lifetime: everything registered in
/// [Scope.authenticated] is created when an account becomes active and disposed
/// when it stops being, so one account's sockets, caches and drafts cannot
/// outlive their account or leak into the next. This is the mechanism that
/// guarantees it, and its registrar is where an account's own services go.
///
/// It follows [AuthManager.activeCredential] — which already folds together
/// signing in, switching accounts, renewing a token and being signed out — and
/// pushes or pops the scope to match:
///
/// * a different account becomes active → the old scope is popped and a new one
///   pushed for the new account;
/// * the same account's tokens are renewed → the scope stays, and only the
///   credential it holds is replaced, so a network refresh never tears down the
///   user's screens or the session running inside them;
/// * nobody is signed in → the scope is gone.
///
/// Transitions are serialised: a sign-out and the sign-in that follows it are
/// applied one after the other rather than interleaved, which is what keeps the
/// scope stack from being pushed twice or popped mid-push. A transition that
/// fails is logged and leaves nothing scoped, so the next account change starts
/// from a clean stack.
final class UserScopeManager implements Disposable {
  /// Creates a manager that follows [_auth].
  new(this._auth, {required this._logger, this._register});

  final AuthManager _auth;
  final MenoLogger _logger;
  final AccountScopeRegistrar? _register;

  ListenableSubscription? _subscription;
  Future<void> _queue = Future<void>.value();
  final _seeded = Completer<void>();
  Id? _scopedUserId;

  /// Subscribes to the active account, then applies the one already in effect.
  ///
  /// Registered with `dependsOn: [AuthManager]`, so the session restored at
  /// startup is in place before this runs and the first scope is pushed for the
  /// right account.
  Future<UserScopeManager> init() async {
    _subscription = _auth.activeCredential.listen(_onActiveCredentialChanged);
    // Deferred by a microtask rather than queued here: this runs while the
    // locator is still starting the container, and pushing a scope from inside
    // another registration's initializer would change the scope stack under it.
    scheduleMicrotask(() {
      unawaited(_transitionTo(_auth.activeCredential.value));
      // Signalled after the queue has it, so [ready] can wait for the scope the
      // restored session pushes.
      _seeded.complete();
    });
    return this;
  }

  /// Resolves once the account that was already signed in has been scoped.
  ///
  /// The app's startup gate waits on this as well as on the container: the
  /// account's own services are registered in the scope this pushes, and a
  /// screen that resolves one must not be built before it exists.
  Future<void> get ready async {
    await _seeded.future;
    await _queue;
  }

  /// Releases the subscription.
  @override
  FutureOr<dynamic> onDispose() async {
    _subscription?.cancel();
    _subscription = null;
  }

  /// Follows the active account.
  void _onActiveCredentialChanged(
    UserCredential credential,
    ListenableSubscription _,
  ) {
    unawaited(_transitionTo(credential));
  }

  /// Applies [credential] once every change queued before it has been.
  Future<void> _transitionTo(UserCredential credential) {
    final next = _queue.then((_) => _apply(credential));
    _queue = next;
    return next;
  }

  /// Puts [credential]'s account in scope, or clears the scope when nobody is
  /// signed in.
  Future<void> _apply(UserCredential credential) async {
    final userId = credential.isEmpty ? null : credential.userId;

    if (userId == _scopedUserId) {
      await _replaceCredential(credential);
      return;
    }

    try {
      await _popScope();
      if (userId == null) return;

      await di.pushNewScopeAsync(
        scopeName: Scope.authenticated,
        init: (scope) async {
          scope.registerSingleton<UserCredential>(credential);
          await _register?.call(scope, credential);
        },
      );
      _scopedUserId = userId;
    } on Object catch (error, stackTrace) {
      // get_it has already taken the half-built scope down by now, so the only
      // thing left to put right is this manager's own idea of what is scoped.
      _scopedUserId = null;
      _logger.w(
        'Could not scope the account $userId',
        tag: MenoLogTag.di.label,
        error: error,
        stackTrace: stackTrace,
      );
    }
  }

  /// Pops the account scope, if one is open — and with it anything that was
  /// pushed on top of the account, such as a live broadcast's session.
  Future<void> _popScope() async {
    _scopedUserId = null;
    if (!di.hasScope(Scope.authenticated)) return;
    await di.popScopesTill(Scope.root, inclusive: false);
  }

  /// Hands the scope the account's refreshed credential.
  ///
  /// The scope is keyed by account, so a renewal keeps it: only the registered
  /// credential is swapped. Never done while another scope is on top — a live
  /// session's, say — because a registration made then would land in it.
  Future<void> _replaceCredential(UserCredential credential) async {
    if (credential.isEmpty) return;
    if (di.currentScopeName != Scope.authenticated) return;
    if (!di.isRegistered<UserCredential>()) return;
    if (di<UserCredential>() == credential) return;

    await di.unregister<UserCredential>();
    di.registerSingleton<UserCredential>(credential);
  }
}
