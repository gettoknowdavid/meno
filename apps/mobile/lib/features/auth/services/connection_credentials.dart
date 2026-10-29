import 'package:meno_mobile/_core/value_objects/id.dart';
import 'package:meno_mobile/features/auth/model/credential_record.dart';

/// The credential read the realtime connection needs.
///
/// Narrow on purpose: the owner asks the vault for one account's usable
/// credential, and `SessionService` is the only implementation. That is what
/// makes a reconnect after a refresh present the *new* token — the value is
/// read when it is needed, never captured with the account.
abstract interface class ConnectionCredentials {
  /// The usable record for [accountId], whichever account is active.
  CredentialRecord? usableCredential(Id accountId);
}
