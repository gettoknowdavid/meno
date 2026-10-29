import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';
import 'package:meno_mobile/features/auth/services/auth_http_service.dart';
import 'package:meno_mobile/features/auth/services/session_service.dart';

/// A manager for the authentication state of the app.
final class AuthManager implements Disposable {
  /// Creates a manager over its service, its store and the log.
  ///
  /// [beginAttempt] and [adoptCandidate] are the transition owner's two
  /// hooks: every sign-in attempt takes a ticket from the owner's single
  /// counter, and the response is handed back for adoption — cancelled when a
  /// newer intent superseded it. When they are absent (a unit test that wires
  /// this manager alone) the record is written straight to the vault, which is
  /// the legacy path and nothing else uses it.
  new({
    required this._http,
    required this._store,
    required this._clock,
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
  final SessionService _store;
  final Clock _clock;
  final MenoLogger _logger;

  /// Asks the transition owner for the next attempt ticket.
  final int Function()? beginAttempt;

  /// Hands a freshly issued credential to the transition owner, which is now
  /// the only code that activates an account.
  final Future<TransitionResult> Function(
    CredentialRecord record, {
    required int ticket,
  })?
  adoptCandidate;

  final _active = ValueNotifier<UserCredential>(UserCredential.empty);
  final _accounts = ValueNotifier<Map<Id, UserCredential>>({});
  final _lastKnown = ValueNotifier<User>(User.empty);
  final _formMode = ValueNotifier<LoginFormMode>(LoginFormMode.fullEmail);

  /// The active account as the vault keeps it.
  ///
  /// The transition owner follows this rather than [activeCredential], because
  /// an epoch and a revision cannot be read back out of a projection, and a
  /// token renewal has to be conditional on exactly those two.
  final _record = ValueNotifier<CredentialRecord?>(null);

  ListenableSubscription? _storeSubscription;

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

  /// The active account as the vault keeps it, or null when signed out.
  ///
  /// Only the transition owner needs this: everything a screen renders comes
  /// from [activeCredential], [accounts] and [lastKnownUser].
  ValueListenable<CredentialRecord?> get activeRecord => _record;

  /// Reads the vault and subscribes to its changes.
  ///
  /// Returns `this` so the locator can register it with the `init()` pattern:
  /// the app's startup gate then waits for a restored session instead of
  /// rendering the sign-in screen for one frame and then navigating away.
  Future<AuthManager> init() async {
    await _store.initialize();
    _startListening();
    await _dropUnrecoverable();
    _applyEnvelope(_store.readSnapshot());
    return this;
  }

  @override
  FutureOr<dynamic> onDispose() {
    _storeSubscription?.cancel();

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

  /// Signs the active account out locally.
  ///
  /// Deliberately not a [Command]: it is the reaction to a session the API has
  /// already refused, so it must not surface a second failure of its own.
  Future<void> signOut() async {
    _logger.i('Signing out locally', tag: MenoLogTag.lifecycle.label);

    final record = _record.value;
    if (record == null) {
      await _store.deactivate();
      _formMode.value = LoginFormMode.prefillSelectedIdentity;
      return;
    }

    // The record itself goes, not just the active pointer: leaving it behind is
    // what makes a logged-out session selectable again later, and the vault
    // refuses to recreate a removed account from a late renewal.
    final result = await _store.removeCredentials(
      record.accountId,
      epoch: record.epoch,
    );
    if (result is MutationStorageFailed) {
      // The device refused the write, so the account is still signed in. Saying
      // otherwise would be a lie the next launch would contradict.
      _logger.w(
        'The account could not be removed from this device, so it stays '
        'signed in',
        tag: MenoLogTag.lifecycle.label,
      );
      throw const StorageException();
    }

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
    Future<AuthResponseDto> Function() request,
  ) async {
    final ticket = beginAttempt?.call() ?? 0;
    final response = await request();

    // Receipt-time conversion, once: the relative lifetime becomes an instant
    // against this one reading of the clock (guide Step 6).
    final record = response.toCredentialRecord(_clock);

    final adopt = adoptCandidate;
    if (adopt != null) {
      final result = await adopt(record, ticket: ticket);
      if (result is TransitionCancelled) {
        throw const CancelledException();
      }
      if (result is TransitionBlocked) {
        throw const ServerException(
          message: 'We could not open this account. Please try again.',
        );
      }
    } else {
      final adopted = await _store.adoptCandidate(record);
      if (adopted is MutationStorageFailed) throw const StorageException();
      await _store.activate(record.accountId);
    }

    // Greeting metadata only, and only once the attempt was admitted: an
    // attempt superseded by a logout must leave nothing on the sign-in screen.
    await _store.updateRememberedIdentity(
      response.toRememberedIdentity(_clock),
    );

    // The sign-in response knows the whole account, so the in-session
    // projection carries it; after a restart the vault's thinner restored one
    // does, and the chrome reads the profile for anything richer (Step 24).
    final credential = record.toUserCredential(user: response.user.toDomain);
    _adopt(credential);
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

  /// Applies the committed vault to everything the UI reads.
  ///
  /// The vault publishes after every successful write, so this is the only
  /// place active state is derived: a sign-in, a renewal, a switch and a logout
  /// all arrive here as one envelope.
  void _applyEnvelope(VaultEnvelope envelope) {
    _accounts.value = _projectAccounts(envelope);

    final activeId = envelope.activeAccountId;
    // `usableCredential` answers null for a session that has to be signed into
    // again — a renewal whose outcome cannot be known, or an expired token with
    // nothing left to renew it with — which is exactly "signed out".
    final record = activeId == null ? null : _store.usableCredential(activeId);

    _record.value = record;
    _adopt(
      record?.toUserCredential(
            remembered: envelope.remembered[record.accountId],
          ) ??
          UserCredential.empty,
    );
    _lastKnown.value = _greeting(envelope);
    if (record != null) {
      _formMode.value = LoginFormMode.prefillSelectedIdentity;
    }
  }

  /// Every account on this device, in the presentation shape.
  Map<Id, UserCredential> _projectAccounts(VaultEnvelope envelope) => {
    for (final record in envelope.credentials.values)
      record.accountId: record.toUserCredential(
        remembered: envelope.remembered[record.accountId],
      ),
  };

  /// The account the signed-out screen greets: the one used most recently.
  ///
  /// Greeting metadata, never authentication: it says who was here, not that
  /// their session is still usable.
  User _greeting(VaultEnvelope envelope) {
    RememberedIdentity? latest;
    for (final identity in envelope.remembered.values) {
      if (latest == null || identity.lastUsedAt.isAfter(latest.lastUsedAt)) {
        latest = identity;
      }
    }
    return latest?.toUser() ?? User.empty;
  }

  /// Drops an active record that nothing can revive.
  ///
  /// An expired access token with no refresh token left is not a session the
  /// app can recover, so it is removed now rather than left to fail the first
  /// request. A refreshable one is kept: the interceptor renews it.
  Future<void> _dropUnrecoverable() async {
    final envelope = _store.readSnapshot();
    final activeId = envelope.activeAccountId;
    if (activeId == null) return;

    final record = envelope.credentials[activeId];
    if (record == null) return;

    final refreshToken = record.refreshToken;
    final refreshable = refreshToken != null && refreshToken.isNotEmpty;
    if (refreshable || record.accessExpiresAt.isAfter(_clock.nowUtc())) return;

    _logger.w(
      'The stored session for ${activeId.orEmpty} expired '
      'and cannot be renewed',
      tag: MenoLogTag.lifecycle.label,
    );
    await _store.removeCredentials(activeId, epoch: record.epoch);
  }

  /// Subscribes to the vault, which is how a token renewed by the interceptor —
  /// or a session it had to drop — reaches the UI.
  void _startListening() {
    _storeSubscription = _store.changes.listen((envelope, _) {
      if (envelope != null) _applyEnvelope(envelope);
    });
  }

  /// Clears the active account so a different one can sign in.
  ///
  /// This is a deactivate, not a logout: every other account stays in the
  /// vault, and the login screen switches to the full form so a second address
  /// can actually be typed.
  Future<void> _addAccount() async {
    final result = await _store.deactivate();
    if (result is MutationStorageFailed) throw const StorageException();
    _formMode.value = LoginFormMode.fullEmail;
  }

  /// Makes [userId] the active account.
  Future<void> _switchAccount(Id userId) async {
    final result = await _store.activate(userId);
    switch (result) {
      case MutationCommitted():
        return;
      case MutationStorageFailed():
        throw const StorageException();
      case MutationStale():
      case MutationRejected():
        throw const NotFoundException(
          message: 'That account is no longer signed in on this device.',
        );
    }
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
