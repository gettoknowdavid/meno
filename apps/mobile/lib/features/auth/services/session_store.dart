import 'dart:async';
import 'dart:convert';

import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/secure_storage.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';

/// Where the accounts signed in on this device live.
///
/// One writer, two readers: the auth manager, the session interceptor and the
/// screens all go through here rather than touching [SecureStorage] keys, so
/// the layout of a stored account is decided in exactly one place.
///
/// The app signs in to several accounts at once and switches between them
/// without a round trip. That needs two things on disk:
///
/// * a **vault** under [StorageKeys.accounts]: every account's whole
///   credential, keyed by user id, which is what makes a switch instant;
/// * an **active** set under [StorageKeys.userId],
///   [StorageKeys.accessToken], [StorageKeys.refreshToken],
///   [StorageKeys.sessionExpiry] and [StorageKeys.credential]: the account
///   every request is made as.
///
/// The active set duplicates one vault entry on purpose. A request needs a
/// bearer token on the hot path, and parsing the whole vault to find it would
/// put a JSON decode in front of every call. The duplication is safe because
/// both are written together here.
///
/// Every mutation publishes the new active credential on [activeChanges]. That
/// is how the auth manager learns that a token was rotated by the interceptor,
/// or that a session was dropped because the API refused to renew it.
///
/// Ids cross this class's public API as [Id] value objects. They are only
/// flattened to strings at the storage boundary (vault keys and the active
/// user id key), and parsed back with [Id.fromString] on the way out.
final class SessionStore implements Disposable {
  new(this._storage);

  final SecureStorage _storage;

  final _activeChanges = StreamController<UserCredential?>.broadcast();

  /// The active credential, published after every mutation.
  ///
  /// Null means no account is signed in. A listener that only cares about
  /// "signed in or not" can compare against null; one that mirrors the user
  /// object gets it from here rather than re-reading storage.
  Stream<UserCredential?> get activeChanges => _activeChanges.stream;

  /// The id of the active account, or null when nobody is signed in.
  ///
  /// A stored value that is not a valid [Id] is treated as no active account.
  Future<Id?> readActiveUserId() async {
    final raw = await _readActiveUserIdRaw();
    if (raw == null) return null;

    final id = Id.fromString(raw);
    return id.isValid ? id : null;
  }

  /// The tokens of the active account.
  ///
  /// Reads only the three token keys, which is why they are kept beside the
  /// vault copy: an interceptor can answer "is there a token, and is it about
  /// to expire" without decoding anything.
  Future<Session> readActiveSession() async {
    final accessToken = await _storage.read(StorageKeys.accessToken);
    if (accessToken == null || accessToken.isEmpty) return Session.empty;

    return Session(
      accessToken: accessToken,
      refreshToken: await _storage.read(StorageKeys.refreshToken),
      expiresAt: await _readExpiry(),
    );
  }

  /// The whole active credential, or null when nobody is signed in.
  Future<UserCredential?> readActiveCredential() async {
    final raw = await _storage.read(StorageKeys.credential);
    if (raw == null || raw.isEmpty) return null;
    return _decode(raw);
  }

  /// The vault entry for [userId], whichever account is active.
  ///
  /// A request that was dispatched as one account must keep using that
  /// account's tokens until it settles, even if the app has switched to
  /// another one since. Reading by the captured id — never by "the active
  /// account" — is what makes that possible.
  Future<UserCredential?> readCredential(Id userId) async {
    if (!userId.isValid) return null;
    return (await _readVault())[userId.getOrCrash];
  }

  /// Every account on this device, keyed by user id.
  ///
  /// An entry that cannot be read is skipped rather than fatal: a vault written
  /// by an older build must not stop the accounts that still parse from being
  /// listed. The same goes for a key that is not a valid [Id].
  Future<Map<Id, UserCredential>> readAccounts() async {
    final vault = await _readVault();

    final accounts = <Id, UserCredential>{};
    for (final entry in vault.entries) {
      final id = Id.fromString(entry.key);
      if (id.isValid) accounts[id] = entry.value;
    }
    return accounts;
  }

  /// Makes [credential] the active account and files it in the vault.
  ///
  /// The order matters: the vault is written first, so a crash between the two
  /// writes leaves an account that can be switched back to.
  Future<void> saveActive(UserCredential credential) async {
    if (credential.isEmpty) return;

    await _writeVaultEntry(credential);
    await _writeActive(credential);
    await _publish();
  }

  /// Files [credential] in the vault without making it active.
  ///
  /// Used when the interceptor renewed an account the user has since switched
  /// away from: the fresh tokens are worth keeping, but not as the active
  /// account.
  Future<void> saveToVault(UserCredential credential) async {
    if (credential.isEmpty) return;

    final key = credential.userId.getOrCrash;
    final vault = await _readVault();

    // A renewal may only update an account that is still on the device. After
    // an explicit logout the entry is gone, and a late renewal must not put it
    // back — that is exactly the recoverable-session defect C03 describes.
    if (!vault.containsKey(key)) return;

    vault[key] = credential;
    await _writeVault(vault);
    if (await _readActiveUserIdRaw() == key) await _publish();
  }

  /// Makes [userId] the active account, when it is in the vault.
  ///
  /// Returns whether the switch happened. A switch is just a copy from the
  /// vault into the active keys, which is why it needs no network call and no
  /// re-authentication.
  Future<bool> activate(Id userId) async {
    if (!userId.isValid) return false;

    final vault = await _readVault();
    final credential = vault[userId.getOrCrash];
    if (credential == null) return false;

    await _writeActive(credential);
    await _publish();
    return true;
  }

  /// Forgets the active account, leaving the vault intact.
  ///
  /// What a revoked or unrenewable session does: the user lands back on the
  /// sign-in screen, and the other accounts on the device are untouched.
  Future<void> clearActive() async {
    await _storage.deleteAllOf([
      StorageKeys.userId,
      StorageKeys.accessToken,
      StorageKeys.refreshToken,
      StorageKeys.sessionExpiry,
      StorageKeys.credential,
    ]);
    await _publish();
  }

  /// Removes [userId] from the vault.
  ///
  /// The active account is cleared with it when it was the active one, so
  /// removing an account never leaves the app acting as an account it no
  /// longer knows.
  Future<void> remove(Id userId) async {
    if (!userId.isValid) return;

    final key = userId.getOrCrash;
    final vault = await _readVault()
      ..remove(key);

    await _writeVault(vault);

    if (await _readActiveUserIdRaw() == key) await clearActive();
  }

  /// Forgets every account on this device.
  Future<void> clear() async {
    await _storage.deleteAllOf([
      StorageKeys.userId,
      StorageKeys.accessToken,
      StorageKeys.refreshToken,
      StorageKeys.sessionExpiry,
      StorageKeys.credential,
      StorageKeys.accounts,
    ]);
    await _publish();
  }

  @override
  FutureOr<dynamic> onDispose() => _activeChanges.close();

  /// The raw stored id of the active account.
  Future<String?> _readActiveUserIdRaw() => _storage.read(StorageKeys.userId);

  /// The active account's expiry, as stored.
  Future<DateTime?> _readExpiry() async {
    final raw = await _storage.read(StorageKeys.sessionExpiry);
    return raw == null ? null : DateTime.tryParse(raw);
  }

  /// The vault as stored: credentials keyed by the raw user id string.
  ///
  /// Kept as strings internally because that is what JSON object keys are;
  /// [readAccounts] is the one place they become [Id]s.
  Future<Map<String, UserCredential>> _readVault() async {
    final raw = await _storage.read(StorageKeys.accounts);
    if (raw == null || raw.isEmpty) return {};

    try {
      final decoded = jsonDecode(raw);
      if (decoded is! Map<String, dynamic>) return {};

      final accounts = <String, UserCredential>{};
      for (final entry in decoded.entries) {
        final stored = entry.value;
        if (stored is! Map<String, dynamic>) continue;
        final credential = _decodeMap(stored);
        if (credential != null) accounts[entry.key] = credential;
      }
      return accounts;
    } on Object {
      return {};
    }
  }

  /// Persists [vault], deleting the key when it is empty.
  Future<void> _writeVault(Map<String, UserCredential> vault) async {
    if (vault.isEmpty) {
      await _storage.delete(StorageKeys.accounts);
      return;
    }

    await _storage.write(
      StorageKeys.accounts,
      value: jsonEncode({
        for (final entry in vault.entries)
          entry.key: entry.value.toDto.toJson(),
      }),
    );
  }

  /// Writes the active keys for [credential].
  Future<void> _writeActive(UserCredential credential) {
    final session = credential.session;
    final expiry = session.expiresAt;

    return _storage.writeAll({
      StorageKeys.userId: credential.userId.getOrCrash,
      StorageKeys.accessToken: session.accessToken,
      StorageKeys.credential: jsonEncode(credential.toDto.toJson()),
      StorageKeys.refreshToken: ?session.refreshToken,
      if (expiry != null)
        StorageKeys.sessionExpiry: expiry.toUtc().toIso8601String(),
    });
  }

  /// Writes [credential] into the vault, leaving every other account alone.
  Future<void> _writeVaultEntry(UserCredential credential) async {
    final vault = await _readVault();
    vault[credential.userId.getOrCrash] = credential;
    await _writeVault(vault);
  }

  /// Publishes the active credential to [activeChanges].
  Future<void> _publish() async {
    if (_activeChanges.isClosed) return;
    _activeChanges.add(await readActiveCredential());
  }

  /// Reads a stored credential from its JSON text.
  UserCredential? _decode(String raw) {
    try {
      final decoded = jsonDecode(raw);
      if (decoded is! Map<String, dynamic>) return null;
      return _decodeMap(decoded);
    } on Object {
      return null;
    }
  }

  /// Reads a stored credential from its JSON map.
  UserCredential? _decodeMap(Map<String, dynamic> json) {
    try {
      return AuthResponseDto.fromJson(json).toDomain;
    } on Object {
      // A vault entry the app can no longer read is not worth failing the whole
      // list over: the account simply will not appear, and signing in again
      // rewrites it.
      return null;
    }
  }
}
