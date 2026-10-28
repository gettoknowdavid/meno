import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';
import 'package:meno_mobile/features/auth/services/auth_http_service.dart';
import 'package:meno_mobile/features/auth/services/session_store.dart';

/// A manager for the authentication state of the app.
final class AuthManager implements Disposable {
  /// Creates a manager over its service, its store and the log.
  ///
  /// [beginAttempt] and [adoptCandidate] are the transition owner's two
  /// hooks: every sign-in attempt takes a ticket from the owner's single
  /// counter, and the response is handed back for adoption — cancelled when a
  /// newer intent superseded it. When they are absent (a unit test that wires
  /// this manager alone) the credential is written directly, which is the
  /// legacy path and nothing else uses it.
  new({
    required this._http,
    required this._store,
    required this._logger,
    this.beginAttempt,
    this.adoptCandidate,
  }) {
    login = .createAsync<LoginArgs, UserCredential>(
      (args) => _accept(
        () => _http.login(
          email: args.email.getOrCrash,
          password: args.password.getOrCrash,
        ),
      ),
      initialValue: UserCredential.empty,
      errorFilterFn: menoExceptionFilter,
      debugName: 'auth.login',
    );

    register = .createAsync<RegisterArgs, UserCredential>(
      (args) => _accept(
        () => _http.register(
          fullName: args.fullName.getOrCrash,
          email: args.email.getOrCrash,
          password: args.password.getOrCrash,
        ),
      ),
      initialValue: UserCredential.empty,
      errorFilterFn: menoExceptionFilter,
      debugName: 'auth.register',
    );

    signInWithGoogle = .createAsync<GoogleSignInArgs, UserCredential>(
      (args) => _accept(
        () => _http.signInWithGoogle(
          idToken: args.idToken,
          deviceLabel: args.deviceLabel,
        ),
      ),
      initialValue: UserCredential.empty,
      errorFilterFn: menoExceptionFilter,
      debugName: 'auth.signInWithGoogle',
    );

    logout = .createAsyncNoParamNoResult(
      _logout,
      errorFilterFn: menoExceptionFilter,
      debugName: 'auth.logout',
    );

    logOutEverywhere = .createAsyncNoParamNoResult(
      _logOutEverywhere,
      errorFilterFn: menoExceptionFilter,
      debugName: 'auth.logOutEverywhere',
    );

    switchAccount = .createAsyncNoResult<Id>(
      _switchAccount,
      errorFilterFn: menoExceptionFilter,
      debugName: 'auth.switchAccount',
    );

    addAccount = .createAsyncNoParamNoResult(
      _addAccount,
      errorFilterFn: menoExceptionFilter,
      debugName: 'auth.addAccount',
    );

    verifyEmail = .createAsync<VerifyEmailArgs, UserCredential>(
      (args) => _accept(
        () => _http.verifyEmail(email: args.email.getOrCrash, code: args.code),
      ),
      initialValue: UserCredential.empty,
      errorFilterFn: menoExceptionFilter,
      debugName: 'auth.verifyEmail',
    );

    resendOtp = .createAsyncNoResult<OtpRequestArgs>(
      (args) => _http.resendOtp(email: args.email.getOrCrash, type: args.type),
      errorFilterFn: menoExceptionFilter,
      debugName: 'auth.resendOtp',
    );

    forgotPassword = .createAsyncNoResult<Email>(
      (email) => _http.forgotPassword(email.getOrCrash),
      errorFilterFn: menoExceptionFilter,
      debugName: 'auth.forgotPassword',
    );

    resetPassword = .createAsyncNoResult<ResetPasswordArgs>(
      (args) => _http.resetPassword(
        email: args.email.getOrCrash,
        code: args.code,
        newPassword: args.newPassword.getOrCrash,
      ),
      errorFilterFn: menoExceptionFilter,
      debugName: 'auth.resetPassword',
    );

    loadSessions = .createAsyncNoParam<List<SessionInfo>>(
      _http.sessions,
      initialValue: const [],
      errorFilterFn: menoExceptionFilter,
      debugName: 'auth.loadSessions',
    );

    revokeSession = .createAsyncNoResult<Id>(
      _http.revokeSession,
      errorFilterFn: menoExceptionFilter,
      debugName: 'auth.revokeSession',
    );
  }

  final AuthHttpService _http;
  final SessionStore _store;
  final MenoLogger _logger;

  /// Asks the transition owner for the next attempt ticket.
  final int Function()? beginAttempt;

  /// Hands a freshly issued credential to the transition owner, which is now
  /// the only code that activates an account.
  final Future<TransitionResult> Function(
    UserCredential credential, {
    required int ticket,
  })?
  adoptCandidate;

  final _active = ValueNotifier<UserCredential>(UserCredential.empty);
  final _accounts = ValueNotifier<Map<Id, UserCredential>>({});
  final _lastKnown = ValueNotifier<User>(User.empty);
  final _formMode = ValueNotifier<LoginFormMode>(LoginFormMode.fullEmail);

  StreamSubscription<UserCredential?>? _storeSubscription;

  /// Signs an account in with an email address and a password.
  late final Command<LoginArgs, UserCredential> login;

  /// Creates an account. The API signs it in as part of registering.
  late final Command<RegisterArgs, UserCredential> register;

  /// Signs an account in with a Google ID token from the platform SDK.
  late final Command<GoogleSignInArgs, UserCredential> signInWithGoogle;

  /// Ends the active account's session, on this device and on the server.
  late final Command<void, void> logout;

  /// Ends every session the active account has.
  late final Command<void, void> logOutEverywhere;

  /// Makes another account in the vault the active one.
  late final Command<Id, void> switchAccount;

  /// Clears the active account so a different one can sign in.
  ///
  /// The vault is left alone: "add an account" must never sign the others out,
  /// which is what makes switching back afterwards instant.
  late final Command<void, void> addAccount;

  /// Confirms the address with a six-digit code.
  late final Command<VerifyEmailArgs, UserCredential> verifyEmail;

  /// Sends a fresh one-time code.
  late final Command<OtpRequestArgs, void> resendOtp;

  /// Sends a password reset code.
  late final Command<Email, void> forgotPassword;

  /// Sets a new password using a reset code.
  late final Command<ResetPasswordArgs, void> resetPassword;

  /// The devices the active account is signed in on.
  late final Command<void, List<SessionInfo>> loadSessions;

  /// Ends one device's session.
  late final Command<Id, void> revokeSession;

  /// Every account on this device, keyed by user id.
  ///
  /// This is what a switch-account sheet renders: the active account plus
  /// the others, with no request in between.
  ValueListenable<Map<Id, UserCredential>> get accounts => _accounts;

  /// The account every request is made as, or [UserCredential.empty].
  ValueListenable<UserCredential> get activeCredential => _active;

  /// Whether the active account's address has been confirmed.
  ///
  /// A `late final` field rather than a getter: a `select` built per access is
  /// a new notifier every call, which no watcher can keep subscribed to (L02).
  late final ValueListenable<bool> emailVerified = _active.select(
    (credential) => credential.user.verified,
  );

  /// Whether an account is signed in.
  ///
  /// Derived, so there is no second flag that can disagree with
  /// [activeCredential]. Cached once, for the same reason as [emailVerified].
  late final ValueListenable<bool> isAuthenticated = _active.select(
    (credential) => !credential.isEmpty,
  );

  /// The last account that signed in, kept across a sign-out.
  ///
  /// A sign-in screen uses it to greet the returning user and to offer the
  /// accounts already on the device before any request is made.
  ValueListenable<User> get lastKnownUser => _lastKnown;

  /// Which form the signed-out login screen shows.
  ///
  /// A decision the auth flow makes: a remembered identity is a greeting, but
  /// "use another account" must always show an editable, empty address field.
  ValueListenable<LoginFormMode> get loginFormMode => _formMode;

  /// Adopts the stored accounts and subscribes to storage changes.
  /// Returns `this` so the locator can register it with the `init()` pattern:
  /// the app's startup gate then waits for a restored session instead of
  /// rendering the sign-in screen for one frame and then navigating away.
  Future<AuthManager> init() async {
    _startListening();
    await _restore();
    return this;
  }

  @override
  FutureOr<dynamic> onDispose() {
    unawaited(_storeSubscription?.cancel());

    login.dispose();
    register.dispose();
    signInWithGoogle.dispose();
    logout.dispose();
    logOutEverywhere.dispose();
    switchAccount.dispose();
    addAccount.dispose();
    verifyEmail.dispose();
    resendOtp.dispose();
    forgotPassword.dispose();
    resetPassword.dispose();
    loadSessions.dispose();
    revokeSession.dispose();

    _active.dispose();
    _accounts.dispose();
    _lastKnown.dispose();
    _formMode.dispose();
  }

  /// Re-reads the vault.
  ///
  /// Cheap (one secure-storage read) and called after every mutation that can
  /// change it, so [accounts] is never a stale snapshot of the sheet.
  Future<void> refreshAccounts() async {
    _accounts.value = await _store.readAccounts();
  }

  /// Signs the active account out locally.
  ///
  /// Deliberately not a [Command]: it is the reaction to a session the API has
  /// already refused, so it must not surface a second failure of its own.
  Future<void> signOut() async {
    _logger.i('Signing out locally', tag: MenoLogTag.lifecycle.label);

    final credential = _active.value;
    if (credential.isEmpty) {
      await _store.clearActive();
      _adopt(UserCredential.empty);
      return;
    }

    // Remove the vault entry, not just the active keys. Leaving the vault copy
    // behind is what makes a logged-out session selectable again later; the
    // store also refuses to recreate a removed account from a late renewal.
    await _store.remove(credential.userId);
    _adopt(UserCredential.empty);
    await refreshAccounts();

    // The user is still the last person who signed in, so the sign-in screen
    // can greet them. That is presentation, not authentication.
    _formMode.value = LoginFormMode.prefillSelectedIdentity;
  }

  /// Runs [request] and adopts its account through the transition owner.
  ///
  /// The ticket is taken *before* the request goes out, so a response that
  /// lands after a newer intent (a logout, a switch) settles as
  /// [TransitionCancelled] and writes nothing — the race audit C02 describes.
  Future<UserCredential> _accept(
    Future<UserCredential> Function() request,
  ) async {
    final ticket = beginAttempt?.call() ?? 0;
    final credential = await request();

    final adopt = adoptCandidate;
    if (adopt != null) {
      final result = await adopt(credential, ticket: ticket);
      if (result is TransitionCancelled) {
        throw const CancelledException();
      }
      if (result is TransitionBlocked) {
        throw const ServerException(
          message: 'We could not open this account. Please try again.',
        );
      }
    } else {
      await _store.saveActive(credential);
    }

    _adopt(credential);
    await refreshAccounts();
    _formMode.value = LoginFormMode.prefillSelectedIdentity;
    return credential;
  }

  /// Publishes [credential] as the active account.
  ///
  /// Idempotent: the store publishes after every write, so this runs twice for
  /// a sign-in (once from the command, once from the stream) and the second run
  /// must not rebuild the app for nothing.
  void _adopt(UserCredential credential) {
    if (_active.value == credential) return;

    _active.value = credential;
    if (credential.isEmpty) return;

    _lastKnown.value = credential.user;
  }

  /// Ends the active session on the server, then locally.
  ///
  /// The local sign-out happens even when the API call fails: "sign out" is an
  /// instruction, not a request, and leaving a user signed in because the
  /// network was down would be the wrong way to fail. The tokens are still
  /// handed to the API when it answers, so the session is genuinely revoked
  /// rather than merely forgotten.
  Future<void> _logout() async {
    // Capture the tokens before the local record is removed: the server call
    // below may use exactly these and nothing else.
    final credential = _active.value;

    // Local removal comes first. "Sign out" is an instruction, not a request;
    // a failure here must be reported as a retry state, never hidden behind a
    // successful-looking network round trip.
    await signOut();

    if (!credential.hasUsableSession) return;

    try {
      await _http.logout(
        refreshToken: credential.session.refreshToken ?? '',
        accessToken: credential.session.accessToken,
      );
    } on MenoException catch (error) {
      _logger.w(
        'The API could not end the session (${error.code}); '
        'the local credentials are already gone',
        tag: MenoLogTag.lifecycle.label,
      );
    }
  }

  /// Ends every session the account has, then signs out locally.
  Future<void> _logOutEverywhere() async {
    try {
      await _http.logOutEverywhere();
    } on MenoException catch (error) {
      _logger.w(
        'The API could not end every session (${error.code}); '
        'signing out locally anyway',
        tag: MenoLogTag.lifecycle.label,
      );
    }

    await signOut();
  }

  /// Loads what is on disk, if anything.
  Future<void> _restore() async {
    await refreshAccounts();

    final credential = await _store.readActiveCredential();
    if (credential == null || credential.isEmpty) return;

    // The account is remembered either way, so the sign-in screen can greet the
    // returning user and pre-fill their address.
    _lastKnown.value = credential.user;

    if (credential.session.isExpired && !credential.session.isRefreshable) {
      // Nothing can bring this session back — there is no refresh token left —
      // so it is cleared now rather than left to fail on the first request.
      _logger.w(
        'The stored session for ${credential.userId.orEmpty} expired '
        'and cannot be renewed',
        tag: MenoLogTag.lifecycle.label,
      );
      await _store.clearActive();
      return;
    }

    _adopt(credential);
    _formMode.value = LoginFormMode.prefillSelectedIdentity;
  }

  /// Subscribes to the store, which is how a token renewed by the interceptor —
  /// or a session it had to drop — reaches the UI.
  void _startListening() {
    _storeSubscription = _store.activeChanges.listen(
      (credential) => _adopt(credential ?? UserCredential.empty),
    );
  }

  /// Clears the active account so a different one can sign in.
  ///
  /// This is a deactivate, not a logout: every other account stays in the
  /// vault, and the login screen switches to the full form so a second address
  /// can actually be typed.
  Future<void> _addAccount() async {
    await _store.clearActive();
    _adopt(UserCredential.empty);
    _formMode.value = LoginFormMode.fullEmail;
  }

  /// Makes [userId] the active account.
  Future<void> _switchAccount(Id userId) async {
    final switched = await _store.activate(userId);
    if (!switched) {
      throw const NotFoundException(
        message: 'That account is no longer signed in on this device.',
      );
    }
    await refreshAccounts();
  }
}

/// The parameters of [AuthManager.signInWithGoogle].
final class GoogleSignInArgs {
  /// Creates Google sign-in arguments.
  const new({required this.idToken, this.deviceLabel});

  /// The ID token the platform SDK produced.
  final String idToken;

  /// What the user calls this device, for the account's device list.
  final String? deviceLabel;
}

/// The parameters of [AuthManager.login].
final class LoginArgs {
  /// Creates sign-in arguments.
  const new({required this.email, required this.password});

  /// The login address.
  final Email email;

  /// The password, checked only for presence.
  final Password password;
}

/// The parameters of [AuthManager.resendOtp].
final class OtpRequestArgs {
  /// Creates resend arguments.
  const new({required this.email, required this.type});

  /// The address to send to.
  final Email email;

  /// Which kind of code to resend.
  final OtpType type;
}

/// The parameters of [AuthManager.register].
final class RegisterArgs {
  /// Creates registration arguments.
  const new({
    required this.fullName,
    required this.email,
    required this.password,
  });

  /// The display name.
  final FullName fullName;

  /// The login address.
  final Email email;

  /// The chosen password.
  final Password password;
}

/// The parameters of [AuthManager.resetPassword].
final class ResetPasswordArgs {
  /// Creates reset arguments.
  const new({
    required this.email,
    required this.code,
    required this.newPassword,
  });

  /// The address the code was sent to.
  final Email email;

  /// The six-digit reset code.
  final String code;

  /// The replacement password.
  final Password newPassword;
}

/// The parameters of [AuthManager.verifyEmail].
final class VerifyEmailArgs {
  /// Creates verification arguments.
  const new({required this.email, required this.code});

  /// The address the code was sent to.
  final Email email;

  /// The six-digit code.
  final String code;
}
