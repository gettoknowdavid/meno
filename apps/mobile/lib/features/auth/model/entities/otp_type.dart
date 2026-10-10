/// Which kind of one-time code is being asked for or presented.
///
/// The wires are `OtpType` in `apps/api/src/modules/auth/model.rs`, sent as the
/// `otp_type` field of `POST /auth/resend-otp`.
enum OtpType {
  /// A code proving the caller owns the address.
  verifyEmail('verify_email'),

  /// A code permitting a password reset.
  resetPassword('reset_password');

  const OtpType(this.wire);

  /// The exact string the API expects.
  final String wire;
}
