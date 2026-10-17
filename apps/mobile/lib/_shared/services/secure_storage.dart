import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:meno_mobile/_core/_core.dart';

/// Typed access to the device's secure store (Keychain, Keystore).
///
/// This is where the credential the API hands out lives: access token, refresh
/// token, expiry, the decoded credential, the accounts on this device. It is
/// deliberately separate from `LocalStorage` — writes are slower and the store
/// is small, so the app must not read secrets on every frame.
///
/// Reads always hit the platform, which is the only source of truth. Mutations
/// notify listeners so the managers that own a secret (auth, push) can react
/// without this service knowing what the secrets mean.
///
/// Keys come from [StorageKeys].
final class SecureStorage extends ChangeNotifier implements Disposable {
  /// Creates secure storage over [_storage].
  new(this._storage);

  final FlutterSecureStorage _storage;

  /// Reads [key], or null when it is not set.
  Future<String?> read(String key) => _guard(() => _storage.read(key: key));

  /// Writes [value] under [key].
  Future<void> write(String key, {required String value}) async {
    await _guard(() => _storage.write(key: key, value: value));
    notifyListeners();
  }

  /// Writes every entry of [values].
  ///
  /// Saving a session touches several keys at once, and the caller cares that
  /// all of them landed before the app carries on.
  Future<void> writeAll(Map<String, String> values) async {
    if (values.isEmpty) return;
    await _guard(
      () => Future.wait([
        for (final entry in values.entries)
          _storage.write(key: entry.key, value: entry.value),
      ]),
    );
    notifyListeners();
  }

  /// Whether [key] holds a value.
  Future<bool> containsKey(String key) =>
      _guard(() => _storage.containsKey(key: key));

  /// Removes [key].
  Future<void> delete(String key) async {
    await _guard(() => _storage.delete(key: key));
    notifyListeners();
  }

  /// Removes every key in [keys].
  Future<void> deleteAllOf(Iterable<String> keys) async {
    final keyList = keys.toList();
    if (keyList.isEmpty) return;
    await _guard(
      () => Future.wait([for (final key in keyList) _storage.delete(key: key)]),
    );
    notifyListeners();
  }

  /// Removes every stored value.
  ///
  /// Used when a session ends and nothing on the device may outlive it.
  Future<void> deleteAll() async {
    await _guard(_storage.deleteAll);
    notifyListeners();
  }

  @override
  FutureOr<dynamic> onDispose() {
    dispose();
  }

  /// Runs [operation], reporting a platform failure as a [StorageException].
  Future<T> _guard<T>(Future<T> Function() operation) async {
    try {
      return await operation();
    } on PlatformException catch (error) {
      throw StorageException(
        cause: error,
        message: 'The device could not reach its secure storage.',
      );
    } on MissingPluginException catch (error) {
      throw StorageException(cause: error);
    }
  }
}
