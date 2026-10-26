/// Which form a signed-out login screen shows.
///
/// The mode is a decision the auth flow makes, never an inference from "is a
/// user remembered?". A remembered identity is enough to greet someone, but
/// adding an account must always show an editable, empty address field.
enum LoginFormMode {
  /// A full, editable address field: the user is signing in as someone new.
  fullEmail,

  /// A remembered identity is shown; only the password is needed.
  prefillSelectedIdentity,
}
