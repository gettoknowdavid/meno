import 'dart:async';
import 'dart:convert';

import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
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
  Future<Id?> readActiveUserId() async {
    final userIdStr = await _storage.read(StorageKeys.userId);
    return userIdStr != null ? Id.fromString(userIdStr) : null;
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

  /// Every account on this device, keyed by user id.
  ///
  /// An entry that cannot be read is skipped rather than fatal: a vault written
  /// by an older build must not stop the accounts that still parse from being
  /// listed.
  Future<Map<Id, UserCredential>> readAccounts() async {
    final raw = await _storage.read(StorageKeys.accounts);
    if (raw == null || raw.isEmpty) return {};

    try {
      final decoded = jsonDecode(raw);
      if (decoded is! Map<Id, dynamic>) return {};

      final accounts = <Id, UserCredential>{};
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

    await _writeVaultEntry(credential);
    if (await readActiveUserId() == credential.userId) await _publish();
  }

  /// Makes [userId] the active account, when it is in the vault.
  ///
  /// Returns whether the switch happened. A switch is just a copy from the
  /// vault into the active keys, which is why it needs no network call and no
  /// re-authentication.
  Future<bool> activate(Id userId) async {
    final accounts = await readAccounts();
    final credential = accounts[userId];
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
    final accounts = await readAccounts()
      ..remove(userId);

    if (accounts.isEmpty) {
      await _storage.delete(StorageKeys.accounts);
    } else {
      await _storage.write(
        StorageKeys.accounts,
        value: jsonEncode({
          for (final entry in accounts.entries)
            entry.key: entry.value.toDto.toJson(),
        }),
      );
    }

    if (await readActiveUserId() == userId) await clearActive();
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

  /// The active account's expiry, as stored.
  Future<DateTime?> _readExpiry() async {
    final raw = await _storage.read(StorageKeys.sessionExpiry);
    return raw == null ? null : DateTime.tryParse(raw);
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

  /// Reads one vault entry from its JSON text.
  ///
  /// Returns null when the stored row is not the shape a credential expects.

  /// Writes [credential] into the vault, leaving every other account alone.
  Future<void> _writeVaultEntry(UserCredential credential) async {
    final accounts = await readAccounts();
    accounts[credential.userId] = credential;

    await _storage.write(
      StorageKeys.accounts,
      value: jsonEncode({
        for (final entry in accounts.entries)
          entry.key: entry.value.toDto.toJson(),
      }),
    );
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
