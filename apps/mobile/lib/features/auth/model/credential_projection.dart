import 'package:meno_mobile/_core/value_objects/_value_objects.dart';
import 'package:meno_mobile/_shared/model/_model.dart';
import 'package:meno_mobile/features/auth/model/credential_record.dart';
import 'package:meno_mobile/features/auth/model/entities.dart';
import 'package:meno_mobile/features/auth/model/session.dart';

/// The app-facing view of what the vault keeps.
///
/// The vault stores the parts a request needs — a token pair, an absolute
/// expiry, an epoch and a revision — and the greeting metadata in
/// [RememberedIdentity]. It deliberately keeps no whole `User`: name, address
/// and avatar belong to the profile, which the signed-in chrome reads directly
/// (guide Step 24). A credential rebuilt from storage therefore carries exactly
/// the greeting-level user, while a credential built from a sign-in response
/// carries the whole account.
extension CredentialRecordX on CredentialRecord {
  /// The token pair a request travels with.
  Session get session => Session(
    accessToken: accessToken,
    refreshToken: refreshToken,
    expiresAt: accessExpiresAt,
  );

  /// This record as the app's credential.
  ///
  /// [user] is passed only when a sign-in response just told us the whole
  /// account; everything else answers from [remembered].
  UserCredential toUserCredential({
    RememberedIdentity? remembered,
    User? user,
  }) => UserCredential(
    session: session,
    user:
        user ??
        remembered?.toUser(verified: isVerified) ??
        User(
          id: accountId,
          fullName: FullName.empty,
          email: Email.empty,
          verified: isVerified,
        ),
  );
}

/// The greeting metadata as something a screen can render.
extension RememberedIdentityX on RememberedIdentity {
  /// The account this identity names, with only the fields it kept.
  ///
  /// Never a token, and never a claim about the session: [verified] is the
  /// caller's answer, because the identity does not store it.
  User toUser({bool verified = false}) => User(
    id: accountId,
    fullName: FullName.fromWire(displayName ?? ''),
    email: Email.fromWire(email ?? ''),
    verified: verified,
    image: ImageInput.fromUrl(avatarUrl),
  );
}
