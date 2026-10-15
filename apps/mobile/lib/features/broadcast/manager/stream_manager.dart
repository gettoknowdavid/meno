import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/manager/live_scope_manager.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';
import 'package:meno_mobile/features/broadcast/services/_services.dart';

/// One broadcast's lobby: what it is, and getting into its room.
///
/// Everything a screen needs before the live shell exists — the broadcast it is
/// about to join and, once the API has answered, the session (and LiveKit
/// token) that joining produced.
///
/// It is the account's, not the room's: a lobby is opened and browsed while no
/// session exists yet, and a join is what opens one. That is why joining also
/// opens the live scope — the token only exists once the API has minted it.
final class StreamManager implements Disposable {
  new({
    required this._http,
    required this._live,
    required this._userId,
    required this._logger,
  }) {
    fetchBroadcast = .createAsyncNoResult<Id>(
      (broadcastId) async {
        broadcast.value = await _http.getBroadcast(broadcastId.getOrCrash);
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'broadcast.fetchBroadcast',
    );

    joinBroadcast = .createAsyncNoResult<Id>(
      (broadcastId) async {
        _isRunning.value = true;
        try {
          final joined = (await _http.join(broadcastId.getOrCrash))
              .toDomain(_userId.getOrCrash);
          session.value = joined;
          broadcast.value = joined.broadcast;
          _logger.d(
            'Joined broadcast $broadcastId as a listener',
            tag: MenoLogTag.network.label,
          );
          await _live.start(broadcastId, session: joined);
        } finally {
          _isRunning.value = false;
        }
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'broadcast.joinBroadcast',
    );
  }

  final BroadcastHttpService _http;
  final LiveScopeManager _live;
  final Id _userId;
  final MenoLogger _logger;

  /// The broadcast the lobby is about.
  final broadcast = ValueNotifier<Broadcast>(Broadcast.empty);

  /// The session joining produced, or null before a join.
  final session = ValueNotifier<BroadcastSession?>(null);

  /// `GET /broadcasts/{id}` for the lobby.
  late final Command<Id, void> fetchBroadcast;

  /// `POST /broadcasts/{id}/join`, answering with the session to enter with.
  ///
  /// The join is also what opens the room's scope: the session is the only
  /// thing carrying a token, so the registrations that need one can only happen
  /// once this answers.
  late final Command<Id, void> joinBroadcast;

  /// Whether a read or a join is in flight.
  ValueListenable<bool> get isRunning => _isRunning;
  final _isRunning = ValueNotifier<bool>(false);

  @override
  FutureOr<dynamic> onDispose() {
    broadcast.dispose();
    session.dispose();
    _isRunning.dispose();

    fetchBroadcast.dispose();
    joinBroadcast.dispose();
  }
}
