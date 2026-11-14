import 'dart:convert';

import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';

/// The live session this device is in the middle of, kept for the next start.
///
/// A broadcast outlives the app process. A phone that is killed mid-session
/// leaves a room on the server that still lists this client, and — for a host —
/// a broadcast still on air with nobody publishing into it. What is written
/// here says *which* broadcast and *whose* session that was, nothing more: a
/// media token is never persisted (M18). Recovery always asks the API for a
/// fresh one — [`BroadcastHttpService.resume`] is the endpoint that answers
/// "I am still the host (or a listener) of this room" — so a record here always
/// means "this was still running when the process stopped" and never carries a
/// credential across a restart.
///
/// It belongs to one account: a room is entered with a session minted for one
/// identity, and re-entering it as another would join under the wrong name.
/// [read] therefore also requires the stored owner to be this store's owner —
/// a record for anyone else reads as none, never as something to resume.
final class LiveSessionStore {
  /// Creates a store over [_storage] for the account [_userId] names.
  const new(this._storage, {required this._userId});

  /// The record's shape version; a record written by another version is
  /// unreadable rather than half-read.
  static const int _version = 1;

  final LocalStorage _storage;
  final Id _userId;

  /// The session this account was in, or null when it ended cleanly — or when
  /// the record belongs to another account or cannot be read.
  SavedLiveSession? read() {
    final json = _storage.getString(_key);
    if (json == null) return null;

    try {
      final record = jsonDecode(json) as Map<String, dynamic>;
      if (record['version'] != _version) return null;
      if (record['ownerId'] != _userId.getOrCrash) return null;

      final broadcastId = Id.fromString(record['broadcastId'] as String);
      if (!broadcastId.isValid) return null;

      return SavedLiveSession(
        broadcastId: broadcastId,
        roleHint: record['roleHint'] as String?,
        savedAt: DateTime.parse(record['savedAt'] as String),
      );
    } on Object {
      return null;
    }
  }

  /// Remembers [session] until it ends.
  ///
  /// Only the identity of the session is stored: who owns it, which broadcast,
  /// which side of the room it was on, and when. The token the API minted is
  /// deliberately dropped — it is short-lived anyway, and preferences are not
  /// where a bearer token belongs.
  Future<void> save(BroadcastSession session) {
    return _storage.setString(
      _key,
      value: jsonEncode({
        'version': _version,
        'ownerId': _userId.getOrCrash,
        'broadcastId': session.broadcast.id.getOrCrash,
        'roleHint': session.broadcast.hostId == _userId
            ? _hostRole
            : _listenerRole,
        'savedAt': DateTime.now().toUtc().toIso8601String(),
      }),
    );
  }

  /// Forgets the session.
  Future<void> clear() => _storage.remove(_key);

  static const String _hostRole = 'host';
  static const String _listenerRole = 'listener';

  String get _key =>
      StorageKeys.forUser(StorageKeys.liveSession, _userId.getOrCrash);
}

/// What a previous process left behind: enough to ask the API to come back,
/// and nothing that could authenticate anything by itself.
final class SavedLiveSession {
  const new({required this.broadcastId, required this.savedAt, this.roleHint});

  final Id broadcastId;

  /// Which side of the room this device was on, when the record says.
  final String? roleHint;

  /// When the record was written — what tells a fresh record from a stale one.
  final DateTime savedAt;

  @override
  String toString() =>
      'SavedLiveSession($broadcastId, role: $roleHint, savedAt: $savedAt)';
}
