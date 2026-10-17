import 'package:freezed_annotation/freezed_annotation.dart';
import 'package:meno_mobile/_core/value_objects/_value_objects.dart';
import 'package:meno_mobile/_shared/model/_model.dart';

part 'entities.freezed.dart';

/// Which kind of one-time code is being asked for or presented.
///
/// The wires are `OtpType` in `apps/api/src/modules/auth/model.rs`, sent as the
/// `otp_type` field of `POST /auth/resend-otp`.
enum OtpType {
  /// A code proving the caller owns the address.
  verifyEmail('verify_email'),

  /// A code permitting a password reset.
  resetPassword('reset_password');

  new(this.wire);

  /// The exact string the API expects.
  final String wire;
}

/// Represents the session information for a user.
///
/// This is used to track the user's session and device information.
@freezed
abstract class SessionInfo with _$SessionInfo {
  const factory({
    required Id id,
    required String deviceLabel,
    DateTime? createdAt,
    DateTime? lastUsedAt,
    int? rotations,
  }) = _SessionInfo;
}

/// Represents a user credential, including the user and their session.
///
/// This is the main entity for the authentication flow.
@freezed
abstract class UserCredential with _$UserCredential {
  const factory({required Session session, required User user}) =
      _UserCredential;

  const new _();

  /// A credential with no account behind it.
  static const UserCredential empty = UserCredential(
    session: Session.empty,
    user: User.empty,
  );

  /// The account id, or an empty string when there is no account.
  Id get userId => user.id;

  /// Whether this is [empty] rather than a real account.
  bool get isEmpty => user.isEmpty;

  /// Whether the tokens can still be used.
  bool get hasUsableSession => session.accessToken.isNotEmpty;
}
