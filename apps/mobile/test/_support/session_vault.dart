import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';
import 'package:meno_mobile/features/auth/services/session_service.dart';
import 'package:meno_mobile/locator.dart';

/// The app's vault, resolved from the locator these tests boot.
SessionService get vault => di<SessionService>();

/// A credential record, in the shape a sign-in response produces at receipt.
///
/// Tests that only need "a signed-in account" use this rather than a full
/// `AuthResponseDto` fixture: the vault's unit is the record, and an account id
/// is the only thing that has to be real.
CredentialRecord testRecord({
  required String accountId,
  String accessToken = 'access-token',
  String? refreshToken = 'refresh-token',
  Duration lifetime = const Duration(hours: 1),
  bool verified = true,
  String epoch = 'epoch-test',
  int revision = 1,
  Clock clock = const SystemClock(),
}) {
  final now = clock.nowUtc();
  return CredentialRecord(
    accountId: Id.fromWire(accountId),
    schemaVersion: CredentialRecord.schemaVersionNumber,
    epoch: epoch,
    revision: revision,
    accessToken: accessToken,
    refreshToken: refreshToken,
    accessExpiresAt: now.add(lifetime),
    receivedAt: now,
    isVerified: verified,
    renewalState: RenewalState.idle,
  );
}

/// Files [record] in the vault and makes it the active account.
///
/// What the sign-in flow does, minus the network: the record is adopted first
/// so a crash between the two writes leaves an account that can be switched
/// back to.
Future<void> seedVault(
  CredentialRecord record, {
  RememberedIdentity? remembered,
}) async {
  final store = vault;
  await store.initialize();
  await store.adoptCandidate(record);
  await store.activate(record.accountId);
  if (remembered != null) await store.updateRememberedIdentity(remembered);
}

/// The greeting metadata a sign-in response would have stored for [accountId].
RememberedIdentity testGreeting({
  required String accountId,
  String? displayName,
  String? email,
  Clock clock = const SystemClock(),
  bool requiresAuthentication = false,
}) => RememberedIdentity(
  accountId: Id.fromWire(accountId),
  displayName: displayName,
  email: email,
  lastUsedAt: clock.nowUtc(),
  requiresAuthentication: requiresAuthentication,
);
