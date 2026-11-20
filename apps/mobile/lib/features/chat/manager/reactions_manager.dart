import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/chat/model/_model.dart';
import 'package:meno_mobile/features/chat/services/chat_socket_service.dart';

/// One reaction to animate, from this device or from the room.
final class ReactionBurst {
  /// Creates one burst.
  const new({required this.id, required this.type, required this.mine});

  /// Unique per burst: the frame's id for a remote reaction, a local counter
  /// for one this device sent.
  final String id;

  /// Which of the five it is.
  final ReactionType type;

  /// Whether this device sent it.
  final bool mine;
}

/// The room's live reactions: what floats over the stage, and how a tap
/// reaches the room.
///
/// # Two directions, one stream
///
/// Everything the person should see is a [ReactionBurst] on [bursts] — their
/// own tap included, queued the instant it happens rather than when the server
/// echoes it. The API broadcasts `newReaction` to the whole room including the
/// sender (`publish_to_room` in `apps/api/src/infrastructure/ws/pubsub.rs`), so
/// a frame from this account is dropped here: drawing it would double every
/// tap.
///
/// # Why only the wire is throttled
///
/// The animation is local and costs nothing, so every tap is drawn. The frame
/// is not free: the API stores a row per reaction and fans it out to every
/// socket in the room, and it has no per-frame rate limit of its own. Sending
/// at most one frame per [sendInterval] keeps a held finger from turning one
/// person's excitement into a broadcast every other client parses, while the
/// trailing send keeps the last tap so a burst still reaches the room.
///
/// # Why [send] is not a Command
///
/// The same reason a keystroke in the composer is not one: there is no result
/// to show, nothing to restrict it while it runs, and a failure worth no more
/// than a log line — the socket may simply be reconnecting. A command here
/// would add an `isRunning` nobody reads and an error surface nobody should
/// see, over a gesture that is meant to feel like it cost nothing.
final class ReactionsManager implements Disposable {
  /// Creates the room's reactions over its socket.
  new({
    required this._socket,
    required this._broadcastId,
    required this._userId,
    required this._logger,
    this.sendInterval = const Duration(milliseconds: 250),
  }) {
    _subscription = _socket.onReaction.listen(_onReaction);
  }

  final ChatSocketChannel _socket;
  final Id _broadcastId;
  final Id _userId;
  final MenoLogger _logger;

  /// The shortest gap between two frames this device sends.
  final Duration sendInterval;

  final _bursts = StreamController<ReactionBurst>.broadcast(sync: true);

  StreamSubscription<ChatReaction>? _subscription;
  Timer? _trailing;
  DateTime? _lastSentAt;
  ReactionType? _pending;
  int _localSeq = 0;
  bool _disposed = false;

  /// Every reaction to animate, this device's and the room's.
  Stream<ReactionBurst> get bursts => _bursts.stream;

  /// Draws [type] at once and, at most once per [sendInterval], sends it.
  ///
  /// Synchronous on purpose: the sprite appears in the frame the tap happened
  /// in, which is what makes the gesture feel answered.
  void send(ReactionType type) {
    if (_disposed) return;

    _emit(ReactionBurst(id: 'local-${_localSeq++}', type: type, mine: true));

    _pending = type;
    final now = DateTime.now();
    final last = _lastSentAt;
    if (last == null || now.difference(last) >= sendInterval) {
      _flush(now);
      return;
    }

    // Inside the window: one trailing frame carries the last tap, so a burst
    // arrives as a rate rather than being swallowed.
    _trailing ??= Timer(sendInterval - now.difference(last), () {
      _trailing = null;
      final pending = _pending;
      if (pending != null && !_disposed) _flush(DateTime.now());
    });
  }

  void _flush(DateTime now) {
    _trailing?.cancel();
    _trailing = null;
    _lastSentAt = now;

    final type = _pending;
    _pending = null;
    if (type == null) return;

    try {
      _socket.sendReaction(broadcastId: _broadcastId, type: type);
    } on Object catch (error, stackTrace) {
      // Best-effort: the socket may be reconnecting, and a lost reaction is not
      // worth a toast over the room.
      _logger.d(
        'ReactionsManager: the reaction was not sent',
        tag: MenoLogTag.chat.label,
        error: error,
        stackTrace: stackTrace,
      );
    }
  }

  /// Animates somebody else's reaction, if this build can draw it.
  void _onReaction(ChatReaction reaction) {
    if (_disposed || reaction.broadcastId != _broadcastId) return;

    // The API fans out to the sender too; this device already drew its taps.
    if (reaction.senderId == _userId) return;

    final type = reaction.type;
    if (type == null) {
      _logger.d(
        'ReactionsManager: dropped a reaction this build cannot draw',
        tag: MenoLogTag.chat.label,
      );
      return;
    }

    _emit(ReactionBurst(id: reaction.id, type: type, mine: false));
  }

  void _emit(ReactionBurst burst) {
    if (_disposed || _bursts.isClosed) return;
    _bursts.add(burst);
  }

  @override
  FutureOr<dynamic> onDispose() async {
    _disposed = true;

    _trailing?.cancel();
    _trailing = null;

    await _subscription?.cancel();
    _subscription = null;

    await _bursts.close();
  }
}
