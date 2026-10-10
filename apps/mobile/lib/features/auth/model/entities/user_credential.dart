import 'package:meno_mobile/_shared/_shared.dart';

/// One account this device is signed in as.
///
/// A credential is what the app keeps per account: the tokens to act as that
/// account, and the user object they describe. The accounts vault holds one of
/// these per id, and the active one is the account every request is made as —
/// which is what makes switching accounts instant instead of a sign-in.
final class UserCredential {
  /// Creates a credential.
  const new({required this.session, required this.user});

  /// The tokens for this account.
  final Session session;

  /// The account the tokens belong to.
  final User user;

  /// A credential with no account behind it.
  static const UserCredential empty = UserCredential(
    session: Session.empty,
    user: User.empty,
  );

  /// The account id, or an empty string when there is no account.
  String get userId => user.id;

  /// Whether this is [empty] rather than a real account.
  bool get isEmpty => user.isEmpty;

  /// Whether the tokens can still be used.
  bool get hasUsableSession => session.accessToken.isNotEmpty;

  /// A copy with the named parts replaced.
  UserCredential copyWith({Session? session, User? user}) => UserCredential(
    session: session ?? this.session,
    user: user ?? this.user,
  );

  @override
  bool operator ==(Object other) =>
      other is UserCredential && other.session == session && other.user == user;

  @override
  int get hashCode => Object.hash(session, user);
}
