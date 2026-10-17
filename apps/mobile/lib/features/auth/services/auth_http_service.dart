import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';

/// The auth module's endpoints, one method each.
final class AuthHttpService {
  /// Creates a service over [_client].
  const new(this._client);

  final HttpClient _client;

  /// `POST /auth/register` — creates an account and signs it in.
  ///
  /// Answers 201 with a usable token pair: the API does not make a new account
  /// sign in again after registering.
  Future<UserCredential> register({
    required String fullName,
    required String email,
    required String password,
  }) {
    return _client.post(
      '/auth/register',
      data: {'full_name': fullName, 'email': email, 'password': password},
      fromJson: _credential,
    );
  }

  /// `POST /auth/login` — exchanges credentials for a token pair.
  Future<UserCredential> login({
    required String email,
    required String password,
  }) {
    return _client.post(
      '/auth/login',
      data: {'email': email, 'password': password},
      fromJson: _credential,
    );
  }

  /// `POST /auth/google/mobile` — exchanges a Google ID token for a session.
  ///
  /// The mobile half of the Google flow: the platform SDK produces the ID
  /// token and the API verifies it, so there is no browser round trip and no
  /// web `state` token involved. [deviceLabel] is what the account's device
  /// list shows later.
  Future<UserCredential> signInWithGoogle({
    required String idToken,
    String? deviceLabel,
  }) {
    return _client.post(
      '/auth/google/mobile',
      data: {'id_token': idToken, 'device_label': ?deviceLabel},
      fromJson: _credential,
    );
  }

  /// `POST /auth/logout` — ends one session.
  ///
  /// Both tokens travel in the body, so this endpoint works without a header
  /// and without a live access token: the refresh token identifies the
  /// session to end.
  Future<void> logout({required String refreshToken, String? accessToken}) {
    return _client.postUnit(
      '/auth/logout',
      data: {'refresh_token': refreshToken, 'access_token': ?accessToken},
    );
  }

  /// `POST /auth/logout-all` — ends every session for the account.
  ///
  /// Guarded: it reads the caller's identity from the request, which is what
  /// makes "all" mean "all of mine".
  Future<void> logOutEverywhere() => _client.postUnit('/auth/logout-all');

  /// `GET /auth/sessions` — the account's signed-in devices.
  Future<List<SessionInfo>> sessions() {
    return _client.getList(
      '/auth/sessions',
      fromJson: (data) =>
          SessionInfoDto.fromJson(data as Map<String, dynamic>).toDomain,
    );
  }

  /// `POST /auth/sessions/{id}` — ends one device's session.
  Future<void> revokeSession(Id sessionId) =>
      _client.postUnit('/auth/sessions/${sessionId.getOrCrash}');

  /// `POST /auth/verify-email` — confirms the address with a code.
  Future<UserCredential> verifyEmail({
    required String email,
    required String code,
  }) {
    return _client.post(
      '/auth/verify-email',
      data: {'email': email, 'code': code},
      fromJson: _credential,
    );
  }

  /// `POST /auth/resend-otp` — sends a fresh code.
  Future<void> resendOtp({required String email, required OtpType type}) {
    return _client.postUnit(
      '/auth/resend-otp',
      data: {'email': email, 'otp_type': type.wire},
    );
  }

  /// `POST /auth/forgot-password` — sends a reset code.
  ///
  /// Answers the same way whether or not the address exists, so the app must
  /// not treat success here as proof that an account was found.
  Future<void> forgotPassword(String email) =>
      _client.postUnit('/auth/forgot-password', data: {'email': email});

  /// `POST /auth/reset-password` — sets a new password with a reset code.
  Future<void> resetPassword({
    required String email,
    required String code,
    required String newPassword,
  }) {
    return _client.postUnit(
      '/auth/reset-password',
      data: {'email': email, 'code': code, 'new_password': newPassword},
    );
  }

  /// Reads the token payload every session-establishing call returns.
  static UserCredential _credential(dynamic data) =>
      AuthResponseDto.fromJson(data as Map<String, dynamic>).toDomain;
}
