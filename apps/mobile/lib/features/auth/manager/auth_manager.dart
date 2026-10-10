import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/model/entities/otp_type.dart';
import 'package:meno_mobile/features/auth/model/entities/session_info.dart';
import 'package:meno_mobile/features/auth/model/entities/user_credential.dart';
import 'package:meno_mobile/features/auth/services/auth_http_service.dart';

/// Every account the app can act as, and what it can do to them.
///
/// The manager owns the *answer* to "who is signed in"; the bytes live in
/// [SessionStore]. That split is what lets the session interceptor renew a token
/// in the background: it writes to the store, the store publishes, and this
/// manager adopts the result — no callback from the transport layer into
/// business logic, and no second copy of the credentials.
///
/// ## Several accounts at once
///
/// [accounts] is the vault — every account signed in on this device — and
/// [activeCredential] is the one every request is made as.
/// [switchAccount] moves between them without a network call, which is the
/// whole point of keeping the vault: a switch is a copy, not a sign-in.
///
/// ## Commands, not methods
///
/// Everything that talks to the API is a [Command], so a screen gets
/// `isRunning`, `errors` and a success signal for free, and every failure takes
/// the same route through [menoExceptionFilter].
final class AuthManager implements Disposable {
  /// Creates a manager over its service, its store and the log.
  AuthManager({
    required AuthHttpService http,
    required SessionStore store,
    required MenoLogger logger,
  }) : _http = http,
       _store = store,
       _logger = logger {
    login = Command.createAsync<LoginArgs, UserCredential>(
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

    register = Command.createAsync<RegisterArgs, UserCredential>(
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

    signInWithGoogle = Command.createAsync<GoogleSignInArgs, UserCredential>(
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

    logout = Command.createAsyncNoParamNoResult(
      _logout,
      errorFilterFn: menoExceptionFilter,
      debugName: 'auth.logout',
    );

    logOutEverywhere = Command.createAsyncNoParamNoResult(
      _logOutEverywhere,
      errorFilterFn: menoExceptionFilter,
      debugName: 'auth.logOutEverywhere',
    );

    switchAccount = Command.createAsyncNoResult<String>(
      _switchAccount,
      errorFilterFn: menoExceptionFilter,
      debugName: 'auth.switchAccount',
    );

    addAccount = Command.createAsyncNoParamNoResult(
      _store.clearActive,
      errorFilterFn: menoExceptionFilter,
      debugName: 'auth.addAccount',
    );

    verifyEmail = Command.createAsync<VerifyEmailArgs, UserCredential>(
      (args) => _accept(
        () => _http.verifyEmail(
          email: args.email.getOrCrash,
          code: args.code,
        ),
      ),
      initialValue: UserCredential.empty,
      errorFilterFn: menoExceptionFilter,
      debugName: 'auth.verifyEmail',
    );

    resendOtp = Command.createAsyncNoResult<OtpRequestArgs>(
      (args) => _http.resendOtp(
        email: args.email.getOrCrash,
        type: args.type,
      ),
      errorFilterFn: menoExceptionFilter,
      debugName: 'auth.resendOtp',
    );

    forgotPassword = Command.createAsyncNoResult<Email>(
      (email) => _http.forgotPassword(email.getOrCrash),
      errorFilterFn: menoExceptionFilter,
      debugName: 'auth.forgotPassword',
    );

    resetPassword = Command.createAsyncNoResult<ResetPasswordArgs>(
      (args) => _http.resetPassword(
        email: args.email.getOrCrash,
        code: args.code,
        newPassword: args.newPassword.getOrCrash,
      ),
      errorFilterFn: menoExceptionFilter,
      debugName: 'auth.resetPassword',
    );

    loadSessions = Command.createAsyncNoParam<List<SessionInfo>>(
      _http.sessions,
      initialValue: const [],
      errorFilterFn: menoExceptionFilter,
      debugName: 'auth.loadSessions',
    );

    revokeSession = Command.createAsyncNoResult<String>(
      _http.revokeSession,
      errorFilterFn: menoExceptionFilter,
      debugName: 'auth.revokeSession',
    );
  }

  final AuthHttpService _http;
  final SessionStore _store;
  final MenoLogger _logger;

  final ValueNotifier<UserCredential> _active = ValueNotifier(
    UserCredential.empty,
  );
  final ValueNotifier<Map<String, UserCredential>> _accounts = ValueNotifier({});
  final ValueNotifier<User> _lastKnown = ValueNotifier(User.empty);

  StreamSubscription<UserCredential?>? _storeSubscription;

  // ======================================================================
  // STATE
  // ======================================================================

  /// The account every request is made as, or [UserCredential.empty].
  ValueListenable<UserCredential> get activeCredential => _active;

  /// Whether an account is signed in.
  ///
  /// Derived, so there is no second flag that can disagree with
  /// [activeCredential].
  ValueListenable<bool> get isAuthenticated =>
      _active.select((credential) => !credential.isEmpty);

  /// Whether the active account's address has been confirmed.
  ValueListenable<bool> get emailVerified =>
      _active.select((credential) => credential.user.verified);

  /// The id of the active account, or null when nobody is signed in.
  ValueListenable<String?> get activeUserId =>
      _active.select((credential) => credential.isEmpty ? null : credential.userId);

  /// Every account on this device, keyed by user id.
  ///
  /// This is what a switch-account sheet renders: the active account plus the
  /// others, with no request in between.
  ValueListenable<Map<String, UserCredential>> get accounts => _accounts;

  /// The last account that signed in, kept across a sign-out.
  ///
  /// A sign-in screen uses it to greet the returning user and to offer the
  /// accounts already on the device before any request is made.
  ValueListenable<User> get lastKnownUser => _lastKnown;

  // ======================================================================
  // COMMANDS
  // ======================================================================

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
  late final Command<String, void> switchAccount;

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
  late final Command<String, void> revokeSession;

  // ======================================================================
  // LIFECYCLE
  // ======================================================================

  /// Adopts the stored accounts and subscribes to storage changes.
  ///
  /// Returns `this` so the locator can register it with the `init()` pattern:
  /// the app's startup gate then waits for a restored session instead of
  /// rendering the sign-in screen for one frame and then navigating away.
  Future<AuthManager> init() async {
    _startListening();
    await _restore();
    return this;
  }

  /// Re-reads the vault.
  ///
  /// Cheap (one secure-storage read) and called after every mutation that can
  /// change it, so [accounts] is never a stale snapshot of the sheet the user is
  /// looking at.
  Future<void> refreshAccounts() async {
    _accounts.value = await _store.readAccounts();
  }

  /// Signs the active account out locally.
  ///
  /// Deliberately not a [Command]: it is the reaction to a session the API has
  /// already refused, so it must not surface a second failure of its own. The
  /// vault is untouched — the account can be switched back to from the sign-in
  /// screen.
  Future<void> signOut() async {
    _logger.i('Signing out locally', tag: MenoLogTag.lifecycle.label);
    await _store.clearActive();
  }

  /// Releases everything the manager holds.
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
  }

  // ======================================================================
  // PRIVATE — session bookkeeping
  // ======================================================================

  /// Runs [request] and makes its account the active one.
  Future<UserCredential> _accept(
    Future<UserCredential> Function() request,
  ) async {
    final credential = await request();

    await _store.saveActive(credential);
    _adopt(credential);
    await refreshAccounts();
    return credential;
  }

  /// Ends the active session on the server, then locally.
  ///
  /// The local sign-out happens even when the API call fails: "sign out" is an
  /// instruction, not a request, and leaving a user signed in because the
  /// network was down would be the wrong way to fail. The tokens are still
  /// handed to the API when it answers, so the session is genuinely revoked
  /// rather than merely forgotten.
  Future<void> _logout() async {
    final credential = _active.value;

    if (credential.hasUsableSession) {
      try {
        await _http.logout(
          refreshToken: credential.session.refreshToken ?? '',
          accessToken: credential.session.accessToken,
        );
      } on MenoException catch (error) {
        _logger.w(
          'The API could not end the session (${error.code}); '
          'signing out locally anyway',
          tag: MenoLogTag.lifecycle.label,
        );
      }
    }

    await signOut();
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

  /// Makes [userId] the active account.
  Future<void> _switchAccount(String userId) async {
    final switched = await _store.activate(userId);
    if (!switched) {
      throw const NotFoundException(
        message: 'That account is no longer signed in on this device.',
      );
    }
    await refreshAccounts();
  }

  /// Subscribes to the store, which is how a token renewed by the interceptor —
  /// or a session it had to drop — reaches the UI.
  void _startListening() {
    _storeSubscription = _store.activeChanges.listen(
      (credential) => _adopt(credential ?? UserCredential.empty),
    );
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
        'The stored session for ${credential.userId} expired and cannot be '
        'renewed',
        tag: MenoLogTag.lifecycle.label,
      );
      await _store.clearActive();
      return;
    }

    _adopt(credential);
  }
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

/// The parameters of [AuthManager.signInWithGoogle].
final class GoogleSignInArgs {
  /// Creates Google sign-in arguments.
  const new({required this.idToken, this.deviceLabel});

  /// The ID token the platform SDK produced.
  final String idToken;

  /// What the user calls this device, for the account's device list.
  final String? deviceLabel;
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

/// The parameters of [AuthManager.resendOtp].
final class OtpRequestArgs {
  /// Creates resend arguments.
  const new({required this.email, required this.type});

  /// The address to send to.
  final Email email;

  /// Which kind of code to resend.
  final OtpType type;
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
