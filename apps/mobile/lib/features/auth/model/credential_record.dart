import 'package:freezed_annotation/freezed_annotation.dart';
import 'package:meno_mobile/_core/value_objects/id.dart';

part 'credential_record.freezed.dart';

enum RenewalState {
  idle,
  inFlight,
  uncertain;

  static RenewalState? tryParse(Object? v) {
    for (final s in values) {
      if (s.name == v) return s;
    }
    return null;
  }
}

@Freezed(fromJson: false, toJson: false)
abstract class CredentialRecord with _$CredentialRecord {
  factory({
    required Id accountId,
    required int schemaVersion,
    required String epoch,
    required int revision,
    required String accessToken,
    required String refreshToken,
    required DateTime accessExpiresAt,
    required DateTime receivedAt,
    required bool isVerified,
    required RenewalState renewalState,
  }) = _CredentialRecord;

  new _();

  static const int schemaVersionNumber = 1;

  static CredentialRecord? tryFromJson(Object? raw) {
    if (raw is! Map<String, Object?>) return null;
    if (raw['schemaVersion'] != schemaVersionNumber) return null;

    final idStr = raw['accountId'] as String?;
    final accountId = idStr != null ? Id.fromString(idStr) : null;
    final epoch = raw['epoch'];
    final revision = raw['revision'];
    final access = raw['accessToken'];
    final refresh = raw['refreshToken'];
    final expiryStr = raw['accessExpiresAt'];
    final expiry = expiryStr is String
        ? DateTime.tryParse(expiryStr)?.toUtc()
        : null;
    final receivedStr = raw['receivedAt'];
    final receivedAt = receivedStr is String
        ? DateTime.tryParse(receivedStr)?.toUtc()
        : null;
    final verified = raw['isVerified'];
    final renewal = RenewalState.tryParse(raw['renewalState']);

    if (accountId == null || expiry == null || renewal == null) return null;
    if (expiry.isBefore(DateTime.now())) return null;
    if (epoch is! String || epoch.isEmpty) return null;
    if (revision is! int || revision < 0) return null;
    if (access is! String || access.isEmpty) return null;
    if (refresh != null && refresh is! String) return null;
    if (verified is! bool) return null;
    if (receivedAt == null) return null;

    return CredentialRecord(
      accountId: accountId,
      schemaVersion: schemaVersionNumber,
      epoch: epoch,
      revision: revision,
      accessToken: access,
      refreshToken: refresh! as String,
      accessExpiresAt: expiry,
      receivedAt: receivedAt,
      isVerified: verified,
      renewalState: renewal,
    );
  }

  Map<String, Object?> toJson() => {
    'accountId': accountId.getOrCrash,
    'schemaVersion': schemaVersion,
    'epoch': epoch,
    'revision': revision,
    'accessToken': accessToken,
    'refreshToken': refreshToken,
    'accessExpiresAt': _iso(accessExpiresAt),
    'receivedAt': _iso(receivedAt),
    'isVerified': isVerified,
    'renewalState': renewalState.name,
  };

  @override
  String toString() =>
      'CredentialRecord($accountId, rev: $revision, '
      '${renewalState.name})';
}

@Freezed(fromJson: false, toJson: false)
abstract class RememberedIdentity with _$RememberedIdentity {
  factory({
    required Id accountId,
    required String? displayName,
    required DateTime lastUsedAt,
    required bool requiresAuthentication,
    String? avatarUrl,
    String? email,
  }) = _RememberedIdentity;

  new _();

  static RememberedIdentity? tryFromJson(Object? raw) {
    if (raw is! Map<String, Object?>) return null;
    final idText = raw['accountId'];
    final accountId = idText is String ? Id.fromString(idText) : null;
    final lastUsedAt = _utc(raw['lastUsedAt']);
    final requiresAuth = raw['requiresAuthentication'];
    final displayName = raw['displayName'];
    final avatarUri = raw['avatarUrl'];
    final email = raw['email'];

    if (accountId == null || lastUsedAt == null) return null;
    if (requiresAuth is! bool) return null;
    if (displayName != null && displayName is! String) return null;
    if (avatarUri != null && avatarUri is! String) return null;
    if (email != null && email is! String) return null;

    return RememberedIdentity(
      accountId: accountId,
      displayName: displayName as String?,
      avatarUrl: avatarUri as String?,
      email: email as String?,
      lastUsedAt: lastUsedAt,
      requiresAuthentication: requiresAuth,
    );
  }

  Map<String, Object?> toJson() => {
    'accountId': accountId.getOrCrash,
    'displayName': displayName,
    'lastUsedAt': _iso(lastUsedAt),
    'requiresAuthentication': requiresAuthentication,
    'avatarUrl': avatarUrl,
    'email': email,
  };
}

@Freezed(fromJson: false, toJson: false)
abstract class VaultEnvelope with _$VaultEnvelope {
  factory({
    required int schemaVersion,
    required int revision,
    required List<Id> orderedAccountIds,
    required Map<Id, CredentialRecord> credentials,
    required Map<Id, RememberedIdentity> remembered,
    Id? activeAccountId,
  }) = _VaultEnvelope;

  new _();

  static const int schemaVersionNumber = 2;

  static VaultEnvelope? tryFromJson(Object? raw) {
    if (raw is! Map<String, Object?>) return null;
    if (raw['schemaVersion'] != schemaVersionNumber) return null;

    final revision = raw['revision'];
    if (revision is! int || revision < 0) return null;

    final orderRaw = raw['orderedAccountIds'];
    if (orderRaw is! List) return null;
    final ordered = <Id>[];
    for (final item in orderRaw) {
      final id = item is String ? Id.fromString(item) : null;
      if (id == null || ordered.contains(id)) return null;
      ordered.add(id);
    }

    final credRaw = raw['credentials'];
    if (credRaw is! Map<String, Object?>) return null;
    final credentials = <Id, CredentialRecord>{};
    for (final e in credRaw.entries) {
      final key = Id.fromString(e.key);
      final record = CredentialRecord.tryFromJson(e.value);
      if (key.isEmpty || record == null || record.accountId != key) return null;
      credentials[key] = record;
    }

    final remRaw = raw['remembered'];
    if (remRaw is! Map<String, Object?>) return null;
    final remembered = <Id, RememberedIdentity>{};
    for (final e in remRaw.entries) {
      final key = Id.fromString(e.key);
      final identity = RememberedIdentity.tryFromJson(e.value);
      if (key.isEmpty || identity == null || identity.accountId != key) {
        return null;
      }
      remembered[key] = identity;
    }

    final activeAccountIdStr = raw['activeAccountId'];
    final activeAccountId = activeAccountIdStr is String
        ? Id.fromString(activeAccountIdStr)
        : null;

    return VaultEnvelope(
      revision: revision,
      schemaVersion: schemaVersionNumber,
      orderedAccountIds: List<Id>.unmodifiable(ordered),
      credentials: Map<Id, CredentialRecord>.unmodifiable(credentials),
      remembered: Map<Id, RememberedIdentity>.unmodifiable(remembered),
      activeAccountId: activeAccountId,
    );
  }

  Map<String, Object?> toJson() => {
    'revision': revision,
    'schemaVersion': schemaVersionNumber,
    'orderedAccountIds': [for (final id in orderedAccountIds) id.value],
    'credentials': {
      for (final e in credentials.entries) e.key.value: e.value.toJson(),
    },
    'remembered': {
      for (final e in remembered.entries) e.key.value: e.value.toJson(),
    },
    'activeAccountId': activeAccountId?.getOrCrash,
  };

  @override
  String toString() =>
      'VaultEnvelope(revision: $revision, '
      'accounts: ${orderedAccountIds.length}, '
      'active: ${activeAccountId?.getOrCrash})';
}

DateTime? _utc(Object? v) => v is String ? DateTime.tryParse(v)?.toUtc() : null;

String _iso(DateTime t) => t.toUtc().toIso8601String();
