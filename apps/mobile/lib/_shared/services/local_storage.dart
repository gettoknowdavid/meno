import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// Typed, reactive access to the device's preferences.
///
/// [SharedPreferences] caches its values once its instance is ready — the app's
/// startup gate waits for that — so every read here is synchronous and cannot
/// fail. Writes notify listeners, which is what lets a widget re-read a key
/// through `watchValue` after another part of the app changed it: only the
/// watchers whose selected value actually changed rebuild.
///
/// Keys come from [StorageKeys]. Anything secret belongs in `SecureStorage`
/// instead — preferences are stored in the clear.
final class LocalStorage extends ChangeNotifier implements Disposable {
  /// Creates storage over [_preferences].
  new(this._preferences);

  final SharedPreferences _preferences;

  /// The keys that currently hold a value.
  Set<String> get keys => _preferences.getKeys();

  /// Whether [key] holds a value.
  bool containsKey(String key) => _preferences.containsKey(key);

  /// The value of [key] as a string, or null when it is not set.
  String? getString(String key) => _preferences.getString(key);

  /// The value of [key] as a bool, or null when it is not set.
  bool? getBool(String key) => _preferences.getBool(key);

  /// The value of [key] as an int, or null when it is not set.
  int? getInt(String key) => _preferences.getInt(key);

  /// The value of [key] as a double, or null when it is not set.
  double? getDouble(String key) => _preferences.getDouble(key);

  /// The value of [key] as a list of strings, or null when it is not set.
  List<String>? getStringList(String key) => _preferences.getStringList(key);

  /// Stores [value] under [key].
  Future<void> setString(String key, {required String value}) =>
      _write(() => _preferences.setString(key, value));

  /// Stores [value] under [key].
  Future<void> setBool(String key, {required bool value}) =>
      _write(() => _preferences.setBool(key, value));

  /// Stores [value] under [key].
  Future<void> setInt(String key, {required int value}) =>
      _write(() => _preferences.setInt(key, value));

  /// Stores [value] under [key].
  Future<void> setDouble(String key, {required double value}) =>
      _write(() => _preferences.setDouble(key, value));

  /// Stores [value] under [key].
  Future<void> setStringList(String key, {required List<String> value}) =>
      _write(() => _preferences.setStringList(key, value));

  /// Removes [key].
  ///
  /// Succeeding on a key that was never set is not an error, but it is not a
  /// change either, so listeners are only told when something was removed.
  Future<void> remove(String key) async {
    final removed = await _guard(() => _preferences.remove(key));
    if (removed) notifyListeners();
  }

  /// Removes every value.
  Future<void> clear() async {
    await _guard(_preferences.clear);
    notifyListeners();
  }

  @override
  FutureOr<dynamic> onDispose() {
    dispose();
  }

  /// Runs a write and tells listeners once it landed.
  Future<void> _write(Future<bool> Function() write) async {
    final written = await _guard(write);
    if (!written) {
      throw const StorageException(message: 'The device refused the write.');
    }
    notifyListeners();
  }

  /// Runs [operation], reporting a storage failure as a [StorageException].
  ///
  /// Preferences fail in two ways: the platform call itself fails, or the
  /// plugin is missing (a test binding, a platform without an implementation).
  /// Both are the same thing to a caller.
  Future<T> _guard<T>(Future<T> Function() operation) async {
    try {
      return await operation();
    } on PlatformException catch (error) {
      throw StorageException(cause: error);
    } on MissingPluginException catch (error) {
      throw StorageException(cause: error);
    }
  }
}
