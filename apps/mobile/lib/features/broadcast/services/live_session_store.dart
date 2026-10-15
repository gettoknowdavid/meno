import 'dart:convert';

import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';

/// The live session this device is in the middle of, kept for the next start.
///
/// A broadcast outlives the app process. A phone that is killed mid-session
/// leaves a room on the server that still lists this client, and — for a host —
/// a broadcast still on air with nobody publishing into it. What is written
/// here is the session the API minted for that room, token included, which is
/// what makes coming back possible without asking the API for a second one; it
/// is cleared the moment the session ends, so a record here always means "this
/// was still running when the process stopped".
///
/// It belongs to one account, like the cache it sits in: a room is entered with
/// a token minted for one identity, and re-entering it as another would join
/// under the wrong name.
final class LiveSessionStore {
  /// Creates a store over [_storage] for the account [_userId] names.
  const new(this._storage, {required this._userId});

  final LocalStorage _storage;
  final Id _userId;

  /// The session this account was in, or null when it ended cleanly.
  ///
  /// It is read back stamped with the moment it was written, because that is
  /// what [BroadcastSession.isExpired] is about and the wire shape carries no
  /// timestamp of its own: a session decoded from the API is stamped "now",
  /// which would make every record look as if it had just been written.
  ///
  /// A record that cannot be decoded reads as none: it would otherwise keep the
  /// app trying to re-enter a room it cannot name, and the next session written
  /// over it replaces it anyway.
  BroadcastSession? read() {
    final json = _storage.getString(_key);
    if (json == null) return null;

    try {
      final record = jsonDecode(json) as Map<String, dynamic>;
      final savedAt = DateTime.parse(record['saved_at'] as String);

      return BroadcastSessionDto.fromJson(
        record['session'] as Map<String, dynamic>,
      ).toDomain(_userId.getOrCrash).copyWith(timestamp: savedAt);
    } on Object {
      return null;
    }
  }

  /// Remembers [session] until it ends.
  Future<void> save(BroadcastSession session) {
    return _storage.setString(
      _key,
      value: jsonEncode({
        'saved_at': DateTime.now().toUtc().toIso8601String(),
        'session': BroadcastSessionDto(
          broadcast: session.broadcast.toDto,
          token: session.token,
        ).toJson(),
      }),
    );
  }

  /// Forgets the session.
  Future<void> clear() => _storage.remove(_key);

  String get _key =>
      StorageKeys.forUser(StorageKeys.liveSession, _userId.getOrCrash);
}
