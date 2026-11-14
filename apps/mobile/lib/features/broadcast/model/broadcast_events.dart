import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';

/// What the socket told the broadcast feature, decoded once.
///
/// A native sealed hierarchy rather than freezed: these are event signals, not
/// serialisable entities — they only ever travel from the adapter to a manager
/// in the same process, and the code generator would add a build step for no
/// benefit.
///
/// One type per event the app consumes, so a manager switches on the kind
/// rather than on a string. Deciding which frame is which, and dropping the
/// ones that do not decode, happens in the adapter (`BroadcastSocketService`),
/// so nothing downstream ever sees a raw map.
sealed class BroadcastEvent {
  const new();
}

/// A broadcast was created or went on air.
final class NewBroadcastEvent extends BroadcastEvent {
  const new({required this.broadcast});

  final Broadcast broadcast;
}

/// A broadcast ended, in any of the ways it can.
///
/// Deliberately *only* the id and the reason. The socket's `endedBroadcast`
/// frame carries nothing else (`WsPayload::ended_broadcast` in
/// `apps/api/src/infrastructure/ws/dto.rs`), so a client that turned it into a
/// full card was inventing a title, a creator and a duration it had never been
/// sent. Whoever reacts to this asks the API for the real summary instead — see
/// the feed's refetch and the ended screen's summary.
final class EndedSignal extends BroadcastEvent {
  const new({required this.broadcastId, required this.reason});

  final Id broadcastId;
  final EndReason reason;
}

/// The home sections the socket told this client to refetch.
final class HomeInvalidatedEvent extends BroadcastEvent {
  const new({required this.invalidation});

  final HomeInvalidatedDto invalidation;
}

/// Somebody joined a room.
final class ParticipantJoinedEvent extends BroadcastEvent {
  const new({required this.participant});

  final BroadcastParticipantEvent participant;
}

/// Somebody left a room.
final class ParticipantLeftEvent extends BroadcastEvent {
  const new({required this.participant});

  final BroadcastParticipantEvent participant;
}

/// The host of a room you are in dropped or came back.
final class HostConnectionChangedEvent extends BroadcastEvent {
  const new({required this.host});

  final HostConnectionEvent host;
}

/// Something went wrong; the server is telling this client directly.
final class BroadcastFailedEvent extends BroadcastEvent {
  const new({required this.error});

  final BroadcastErrorEvent error;
}

/// A cohost handshake or roster change.
///
/// One sealed family because these frames share a manager's interest: who may
/// publish, and with which token. Each variant mirrors a `WsPayload`
/// constructor in `apps/api/src/infrastructure/ws/dto.rs`.
sealed class CohostEvent extends BroadcastEvent {
  const new({required this.broadcastId});

  /// The broadcast the handshake or change belongs to.
  final Id broadcastId;
}

/// The v2 invitation: the invitee decides, so the frame carries who asked and
/// which broadcast instead of a media token they have no right to yet.
final class CohostInvitedEvent extends CohostEvent {
  const new({
    required super.broadcastId,
    required this.invitationId,
    required this.inviterName,
    required this.broadcastTitle,
  });

  /// The invitation to accept or decline.
  final Id invitationId;

  /// Who sent it, for the prompt.
  final String inviterName;

  /// The broadcast's title, for the prompt.
  final String broadcastTitle;
}

/// The invitee accepted; the creator and room learn who joined.
final class CohostAcceptedEvent extends CohostEvent {
  const new({
    required super.broadcastId,
    required this.userId,
    required this.userName,
  });

  final Id userId;
  final String userName;
}

/// The invitee declined.
final class CohostDeclinedEvent extends CohostEvent {
  const new({required super.broadcastId, required this.userId});

  final Id userId;
}

/// A cohost was demoted back to a listener; the token re-authorises that role.
final class CohostDemotionEvent extends CohostEvent {
  const new({required super.broadcastId, required this.livekitToken});

  final String livekitToken;
}

/// The accepted invitee's cohost session: the token they publish with.
final class NewCohostEvent extends CohostEvent {
  const new({
    required super.broadcastId,
    required this.userId,
    required this.userName,
    required this.livekitToken,
  });

  final Id userId;
  final String userName;
  final String livekitToken;
}

/// A cohost was removed from the broadcast entirely.
final class RemovedCohostEvent extends CohostEvent {
  const new({required super.broadcastId, required this.userId});

  final Id userId;
}

/// A broadcast was deleted, so an open page or cached card for it is gone.
final class BroadcastDeletedEvent extends BroadcastEvent {
  const new({required this.broadcastId});

  final Id broadcastId;
}

/// The live-participant count changed without a join or leave frame.
final class LiveParticipantCountEvent extends BroadcastEvent {
  const new({required this.broadcastId, required this.count});

  final Id broadcastId;
  final int count;
}

/// A scheduled broadcast's start time has arrived.
///
/// Carries the full broadcast response, because the frame does — the card
/// can be refreshed from it without a follow-up request.
final class ScheduledBroadcastEvent extends BroadcastEvent {
  const new({required this.broadcast});

  final Broadcast broadcast;
}

/// A recording lifecycle change.
sealed class RecordingEvent extends BroadcastEvent {
  const new({required this.broadcastId});

  final Id broadcastId;
}

/// The recording finished processing and can be played.
final class RecordingReadyEvent extends RecordingEvent {
  const new({required super.broadcastId});
}

/// The creator published the recording.
final class RecordingPublishedEvent extends RecordingEvent {
  const new({required super.broadcastId});
}

/// Somebody joined or left a broadcast.
final class BroadcastParticipantEvent {
  const new({required this.broadcastId, required this.count, this.participant});

  final Id broadcastId;

  /// How many accounts are in the room after the change.
  final int count;
  final Participant? participant;
}

/// The host's socket is in its grace window, or came back.
final class HostConnectionEvent {
  const new({required this.broadcastId, required this.connected});

  final Id broadcastId;
  final bool connected;
}

/// Why the server sent a `broadcastError`, mirroring `WsErrorCode` in
/// `apps/api/src/infrastructure/ws/errors.rs`.
///
/// The wire value is SCREAMING_SNAKE_CASE and is a stability contract: the API
/// may add a code but never rename one, so matching on it is safe.
enum BroadcastErrorCode {
  tokenExpired('TOKEN_EXPIRED'),
  broadcastForciblyEnded('BROADCAST_FORCIBLY_ENDED'),
  kickedFromRoom('KICKED_FROM_ROOM'),
  roomNotFound('ROOM_NOT_FOUND'),
  mediaServerError('MEDIA_SERVER_ERROR'),
  unsupported('UNSUPPORTED');

  new(this.wire);

  /// The exact string the API sends.
  final String wire;

  /// Whether retrying the same action can succeed.
  ///
  /// Mirrors `WsErrorCode::is_recoverable`: the socket flag is derived from
  /// this server-side, so this default only applies when the frame omits it.
  bool get isRecoverable => this == tokenExpired || this == mediaServerError;

  /// Whether this error means the room is gone and the client should leave it.
  ///
  /// Mirrors `WsErrorCode::is_fatal`: an `unsupported` frame is not recoverable
  /// by retrying, but it is also not a reason to drop a working connection.
  bool get isFatal =>
      this == broadcastForciblyEnded ||
      this == kickedFromRoom ||
      this == roomNotFound;

  static BroadcastErrorCode? fromWire(String? wire) {
    for (final code in BroadcastErrorCode.values) {
      if (code.wire == wire) return code;
    }
    return null;
  }
}

/// A `broadcastError` frame, decoded once here.
final class BroadcastErrorEvent {
  const new({
    required this.code,
    required this.message,
    required this.recoverable,
    this.broadcastId,
  });

  /// The room this concerns, when it concerns one. Connection-scoped errors
  /// (a malformed heartbeat, an unsupported frame, a server shutdown) carry
  /// no id.
  final Id? broadcastId;

  /// The stable code to branch on; never match on [message].
  final BroadcastErrorCode code;

  /// Human-readable detail. Safe to show, never used for control flow.
  final String message;

  /// Whether retrying can succeed — the server derives this from [code].
  final bool recoverable;
}
