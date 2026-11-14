import 'package:freezed_annotation/freezed_annotation.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/model/entities.dart';

part 'live_snapshot.freezed.dart';

/// Where one session has got to.
///
/// A single phase rather than parallel booleans: the old manager carried
/// `LiveSessionState`, `LiveStatus` and private leaving flags at once, and two
/// of them could say different things (audit §1.4). Payload-free, so it is a
/// plain enum (D-25).
///
/// The order is the life of a session: idle → connecting → live, with
/// `reconnecting` a live session whose transport dropped, `ending` while the
/// exit runs, and `ended`/`failed` terminal until a new session starts.
enum LivePhase { idle, connecting, live, reconnecting, ending, ended, failed }

/// Why a session could not stay up, in the shape the UI branches on.
///
/// A plain immutable class, not freezed: it carries no generated behaviour and
/// the only thing it needs is equality, so value semantics are written out
/// (D-25's type note keeps freezed to [LiveSnapshot]).
@immutable
final class LiveFailure {
  const new({required this.message, required this.retryable, this.cause});

  /// A failure no retry can fix: a refused token, a room that no longer
  /// exists. The screen offers an exit rather than a retry button.
  const new terminal(this.message) : retryable = false, cause = null;

  /// What to show a person. Never a raw exception string.
  final String message;

  /// Whether another attempt can plausibly succeed. A refused token cannot; a
  /// dropped connection can.
  final bool retryable;

  /// The failure behind [message], for logs — never for branching.
  final Object? cause;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is LiveFailure &&
          runtimeType == other.runtimeType &&
          message == other.message &&
          retryable == other.retryable;

  @override
  int get hashCode => Object.hash(message, retryable);

  @override
  String toString() => 'LiveFailure($message, retryable: $retryable)';
}

/// One authoritative view of the live broadcast this account is in.
///
/// Everything a live screen draws comes from here: [phase] for where the
/// session is, [canPublish] for the microphone, [canEndBroadcast] for the end
/// control, and [endedSummary] for the screen shown after it closed. The
/// banner, the live page and the ended page all read this one value, so they
/// cannot contradict each other the way `state` and `status` could.
///
/// Every field is value-equal, so re-publishing an identical snapshot notifies
/// nobody — a transport event that changes nothing must not rebuild the room.
@freezed
abstract class LiveSnapshot with _$LiveSnapshot {
  const factory({
    required LivePhase phase,

    /// The broadcast being drawn, while there is one to draw.
    Broadcast? broadcast,

    /// This account's standing in the room, as the API resolved it.
    ParticipantRole? role,

    /// Whether this account may publish audio (creator or accepted cohost).
    @Default(false) bool canPublish,

    /// Whether this account may end the broadcast for everyone. Creator only
    /// — deliberately not [canPublish] (audit §3 H08).
    @Default(false) bool canEndBroadcast,

    /// The failure that stopped the session, when one did.
    LiveFailure? failure,

    /// Why it ended, as the API or the socket said.
    EndReason? endReason,

    /// The broadcast as it ended, retained after the session's resources are
    /// gone so the ended page never reads a disposed object.
    Broadcast? endedSummary,
  }) = _LiveSnapshot;

  const new _();

  /// Nothing is open.
  factory idle() => const LiveSnapshot(phase: LivePhase.idle);

  /// A snapshot for a room, with its capabilities derived in one place.
  ///
  /// [canPublish] comes from the server's own answer (a cohost was accepted, a
  /// listener was not); end authority comes from the broadcast's creator
  /// identity. Never the other way round: `isHost = canPublish` is what let a
  /// cohost reach `/end` (H08).
  factory room({
    required LivePhase phase,
    required Broadcast broadcast,
    required Id accountId,
    required bool canPublish,
    ParticipantRole? role,
    LiveFailure? failure,
    EndReason? endReason,
  }) => LiveSnapshot(
    phase: phase,
    broadcast: broadcast,
    role: role ?? broadcast.participantRole,
    canPublish: canPublish,
    canEndBroadcast: broadcast.canEndBroadcast(accountId),
    failure: failure,
    endReason: endReason,
  );

  /// Whether a room is open (or opening), so the leave control applies.
  bool get isInRoom =>
      phase == .connecting ||
      phase == .live ||
      phase == .reconnecting ||
      phase == .ending;

  /// Whether the exit is the creator's end rather than a listener's leave.
  bool get exitsByEnding => canEndBroadcast && isInRoom;

  /// Whether this account should be shown a "Leave" control instead of "End".
  bool get leavesOnly => !canEndBroadcast && isInRoom;
}
