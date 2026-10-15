import 'package:freezed_annotation/freezed_annotation.dart';

part 'entities.freezed.dart';

/// A broadcast as `GET /broadcasts/{id}` answers with.
///
/// The shape is the API's `BroadcastResponse`. The viewer's own standing —
/// [participantRole], [isJoined], [isBookmarked] — is kept here rather than in
/// a second view object, because every screen that draws one needs it.
@freezed
abstract class Broadcast with _$Broadcast {
  const factory({
    required String id,
    required String title,
    String? description,
    String? timeZone,
    String? imageUrl,
    String? imageId,
    DateTime? createdAt,
    DateTime? startTime,
    DateTime? endTime,
    DateTime? publishedAt,
    int? durationSeconds,
    @Default(BroadcastStatus.inactive) BroadcastStatus status,
    @Default(BroadcastState.draft) BroadcastState state,
    @Default(ParticipantRole.unknown) ParticipantRole participantRole,
    @Default(false) bool isJoined,
    @Default(false) bool isBookmarked,
    @Default(0) int liveParticipantsCount,
    @Default(0) int totalParticipants,
    @Default(false) bool recordingEnabled,
    String? recordingUrl,
    @Default(EndReason.none) EndReason endReason,
    int? timeRemainingSeconds,
    Participant? creator,
    @Default(<Participant>[]) List<Participant> cohosts,
  }) = _Broadcast;

  const new _();

  /// A broadcast with nothing behind it: what a screen paints before it loads.
  static const Broadcast empty = Broadcast(id: '', title: '');

  /// Whether the viewer may publish into the room.
  bool get canPublish => participantRole.isHost || participantRole.isCohost;

  /// How long an ended broadcast ran, when the API stated it.
  Duration? get duration =>
      durationSeconds == null ? null : Duration(seconds: durationSeconds!);

  bool get hasEnded => state.hasEnded || endReason != EndReason.none;

  /// The account that owns the broadcast.
  String get hostId => creator?.id ?? '';

  /// The host's avatar, when one was uploaded.
  String? get hostImageUrl => creator?.avatarUrl;

  /// The host's name, for a card with no room for a full participant.
  String get hostName => creator?.fullName ?? '';

  bool get isDraft => state == BroadcastState.draft;

  bool get isEmpty => id.isEmpty;

  /// Whether the broadcast is on air, a reconnecting host included.
  bool get isLive => state.onAir;

  bool get isNotEmpty => id.isNotEmpty;

  bool get isScheduled => state == BroadcastState.scheduled;

  /// How much of a scheduled broadcast is left, when the API stated it.
  Duration? get timeRemaining => timeRemainingSeconds == null
      ? null
      : Duration(seconds: timeRemainingSeconds!);

  bool isHostedBy(String userId) => hostId == userId;
}

/// Which step of creating a broadcast a draft got to.
enum BroadcastCreationStep { none, created, started, saved }

/// A broadcast being written, before it exists anywhere but the device.
@freezed
abstract class BroadcastDraft with _$BroadcastDraft {
  const factory({
    required String id,
    @Default('') String ownerId,
    @Default('') String title,
    @Default('') String description,
    @Default(false) bool record,
    @Default(<String>[]) List<String> cohosts,
    String? imagePath,
    String? imageId,
    String? createdBroadcastId,
    @Default(BroadcastCreationStep.none) BroadcastCreationStep creationStep,
    DateTime? lastModified,
  }) = _BroadcastDraft;

  const new _();

  /// Whether a broadcast was created on the server that this draft still owns.
  bool get hasPartialCreation =>
      creationStep != BroadcastCreationStep.none &&
      (createdBroadcastId?.isNotEmpty ?? false);

  /// Whether the draft has been written to at all.
  bool get isEmpty => title.trim().isEmpty && description.trim().isEmpty;
}

/// Which way a broadcast list is ordered. Mirrors `meno_core::Order`.
enum BroadcastOrder {
  newest('desc'),
  oldest('asc');

  new(this.wire);

  final String wire;
}

/// The filters one broadcast feed is built from.
///
/// Cursor-paginated, like every other list endpoint: the feed holds on to the
/// cursor the previous page answered with and hands it back for the next one,
/// rather than counting pages. Counted pages drift as broadcasts start and end,
/// which is exactly the list this is.
@freezed
abstract class BroadcastQuery with _$BroadcastQuery {
  const factory({
    BroadcastsType? type,
    String? creatorId,
    BroadcastStatus? status,
    BroadcastOrder? order,
    String? cursor,
    @Default(20) int limit,
  }) = _BroadcastQuery;

  const new _();

  /// The query as request parameters, dropping what is not set.
  Map<String, dynamic> toQueryParameters() => {
    'creator_id': ?creatorId,
    'status': ?status?.wire,
    'order': ?order?.wire,
    'cursor': ?cursor,
    'limit': limit,
  };

  /// The same filters, asking for the page [cursor] names.
  ///
  /// A method rather than `copyWith` because freezed's `copyWith` ignores a
  /// `null` argument, and "no cursor" is exactly what the first page means: a
  /// feed that refreshed with `copyWith(cursor: null)` would silently ask for
  /// the page it already has.
  BroadcastQuery withCursor(String? cursor) => BroadcastQuery(
    type: type,
    creatorId: creatorId,
    status: status,
    order: order,
    cursor: cursor,
    limit: limit,
  );
}

/// The session a client joins a live broadcast with.
///
/// `POST /broadcasts/{id}/live` and `POST /broadcasts/{id}/join` answer with the
/// broadcast and the LiveKit token for it. The account it belongs to travels
/// with it, so live code never reaches for the auth manager mid-broadcast.
@freezed
abstract class BroadcastSession with _$BroadcastSession {
  const factory({
    required String userId,
    required Broadcast broadcast,
    required String token,
    @Default(false) bool canPublish,
    DateTime? timestamp,
  }) = _BroadcastSession;

  const new _();

  bool get isEmpty => broadcast.isEmpty || token.isEmpty;

  /// Whether this session has been sitting around long enough to be suspect.
  bool get isExpired {
    final since = timestamp;
    return since != null && DateTime.now().difference(since).inHours > 12;
  }

  bool get isValid => !isEmpty;
}

/// Mirrors `BroadcastState` in `apps/api/src/modules/broadcast/model.rs`.
enum BroadcastState {
  live('live'),
  reconnecting('reconnecting'),
  ended('ended'),
  scheduled('scheduled'),
  draft('draft');

  new(this.wire);

  final String wire;

  bool get hasEnded => this == ended;

  bool get onAir => this == live || this == reconnecting;

  static BroadcastState fromWire(String? wire) {
    for (final state in BroadcastState.values) {
      if (state.wire == wire) return state;
    }
    return BroadcastState.draft;
  }
}

/// Mirrors `BroadcastStatus` in `apps/api/src/modules/broadcast/model.rs`.
enum BroadcastStatus {
  inactive('inactive'),
  active('active'),
  ended('ended');

  new(this.wire);

  final String wire;

  static BroadcastStatus fromWire(String? wire) {
    for (final status in BroadcastStatus.values) {
      if (status.wire == wire) return status;
    }
    return BroadcastStatus.inactive;
  }
}

/// The audience a broadcast list is built for.
///
/// Decides how a live socket event is applied to that list, which is why it
/// travels with the query rather than being inferred from the response.
enum BroadcastsType { forYou, nowLive, recentlyLive }

/// A broadcast as a list answers with it.
///
/// The API's `BroadcastListItem`: enough to draw a card, and deliberately
/// without the viewer's own standing, which only the single-broadcast endpoint
/// resolves. The creator arrives flattened rather than as a nested object.
@freezed
abstract class BroadcastSummary with _$BroadcastSummary {
  const factory({
    required String id,
    required String title,
    String? description,
    String? timeZone,
    String? imageUrl,
    String? imageId,
    @Default(BroadcastStatus.inactive) BroadcastStatus status,
    @Default(BroadcastState.draft) BroadcastState state,
    DateTime? createdAt,
    DateTime? startTime,
    DateTime? endTime,
    @Default(0) int totalParticipants,
    @Default('') String creatorId,
    @Default('') String creatorName,
    String? creatorAvatarId,
    String? creatorAvatarUrl,
  }) = _BroadcastSummary;

  const new _();

  static const BroadcastSummary empty = BroadcastSummary(id: '', title: '');

  bool get hasEnded => state.hasEnded;

  bool get isEmpty => id.isEmpty;

  bool get isLive => state.onAir;

  bool get isScheduled => state == BroadcastState.scheduled;

  bool isHostedBy(String userId) => creatorId == userId;

  /// This summary as the richer type, for code that takes a [Broadcast].
  Broadcast toBroadcast() => Broadcast(
    id: id,
    title: title,
    description: description,
    timeZone: timeZone,
    imageUrl: imageUrl,
    imageId: imageId,
    createdAt: createdAt,
    startTime: startTime,
    endTime: endTime,
    status: status,
    state: state,
    totalParticipants: totalParticipants,
    creator: creatorId.isEmpty
        ? null
        : Participant(
            id: creatorId,
            fullName: creatorName,
            avatarId: creatorAvatarId,
            avatarUrl: creatorAvatarUrl,
          ),
  );
}

/// What a broadcast ended as.
///
/// The API's `EndBroadcastResponse`, which is also what the socket's
/// `endedBroadcast` event carries: the room is gone by then and a screen still
/// has to draw what ended.
@freezed
abstract class EndedBroadcast with _$EndedBroadcast {
  const factory({
    required String broadcastId,
    required String title,
    @Default('') String creatorId,
    @Default('') String creatorName,
    DateTime? endedAt,
    String? imageUrl,
    @Default(EndReason.normal) EndReason reason,
    @Default(Duration.zero) Duration duration,
    @Default(0) int totalParticipants,
    @Default(false) bool recordingEnabled,
  }) = _EndedBroadcast;

  const new _();

  /// The broadcast this ending stands for, for a screen that shows a card.
  Broadcast get details => Broadcast(
    id: broadcastId,
    title: title,
    imageUrl: imageUrl,
    endTime: endedAt,
    endReason: reason,
    status: BroadcastStatus.ended,
    state: BroadcastState.ended,
    durationSeconds: duration.inSeconds,
    totalParticipants: totalParticipants,
    recordingEnabled: recordingEnabled,
    creator: creatorId.isEmpty
        ? null
        : Participant(id: creatorId, fullName: creatorName),
  );

  bool get isEmpty => broadcastId.isEmpty;

  bool isHostedBy(String userId) => creatorId == userId;
}

/// Mirrors `EndReason` in `apps/api/src/modules/broadcast/model.rs`.
enum EndReason {
  normal('normal'),
  hostDisconnected('host_disconnected'),
  adminForced('admin_forced'),
  quotaExceeded('quota_exceeded'),
  none('none');

  new(this.wire);

  final String wire;

  String get label => switch (this) {
    .normal => 'The host ended this broadcast',
    .hostDisconnected => 'The host lost connection',
    .adminForced => 'An administrator ended this broadcast',
    .quotaExceeded => 'The broadcast reached its limit',
    .none => 'Broadcast ended',
  };

  static EndReason fromWire(String? wire) {
    for (final reason in EndReason.values) {
      if (reason.wire == wire) return reason;
    }
    return EndReason.none;
  }
}

/// A broadcast an account keeps for later.
@freezed
abstract class FavouriteBroadcast with _$FavouriteBroadcast {
  const factory({
    required String broadcastId,
    required String ownerId,
    @Default('') String title,
    @Default('') String creatorName,
    String? imageUrl,
    DateTime? savedAt,
  }) = _FavouriteBroadcast;

  const new _();
}

final class LiveSessionBroadcasting extends LiveSessionState {
  const new();
}

final class LiveSessionEnded extends LiveSessionState {
  const new();
}

final class LiveSessionError extends LiveSessionState {
  const new(this.message);

  final String message;
}

final class LiveSessionHostDisconnected extends LiveSessionState {
  const new();
}

final class LiveSessionInitializing extends LiveSessionState {
  const new();
}

/// Where a live session has got to.
sealed class LiveSessionState {
  const new();

  const factory broadcasting() = LiveSessionBroadcasting;

  const factory ended() = LiveSessionEnded;

  const factory error(String message) = LiveSessionError;

  const factory hostDisconnected() = LiveSessionHostDisconnected;

  const factory initializing() = LiveSessionInitializing;

  const factory streaming() = LiveSessionStreaming;
}

final class LiveSessionStreaming extends LiveSessionState {
  const new();
}

/// The connection status a live screen shows.
enum LiveStatus {
  initializing,
  connecting,
  live,
  reconnecting,
  disconnected,
  hostDisconnected,
  disconnecting,
  offAir;

  String get hostTitle => switch (this) {
    .live => 'Now Live',
    .reconnecting => 'Reconnecting',
    _ => '',
  };

  bool get isActive => this == live;

  bool get isInitializing => this == initializing || this == connecting;

  bool get isNotLive => this != live && this != reconnecting;

  String get label => switch (this) {
    .initializing => 'Initializing...',
    .connecting => 'Connecting...',
    .live => 'Live',
    .reconnecting => 'Reconnecting...',
    .disconnected => 'Disconnected',
    .hostDisconnected => 'Host disconnected',
    .disconnecting => 'Ending...',
    .offAir => 'Off air',
  };

  String get participantTitle => switch (this) {
    .live => 'Now Streaming',
    .reconnecting => 'Reconnecting',
    _ => '',
  };

  bool get showReconnecting => this == reconnecting || this == connecting;
}

/// An account as a broadcast sees it.
///
/// One type covers both shapes the API sends: `UserSummary`, which a creator or
/// cohost arrives as, and `ParticipantListItem`, which the participant list
/// uses. They differ only in what is known about the account — a summary has no
/// role or join time because it is not in the room — so the absent fields are
/// null here rather than a second type.
@freezed
abstract class Participant with _$Participant {
  const factory({
    required String id,
    required String fullName,
    String? avatarId,
    String? avatarUrl,
    String? bio,
    @Default(ParticipantRole.unknown) ParticipantRole role,
    @Default(false) bool isJoined,
    DateTime? joinedAt,
  }) = _Participant;

  const new _();

  static const Participant empty = Participant(id: '', fullName: '');

  /// Whether the account may publish into the room.
  bool get canPublish => isHost || isCohost;

  bool get isCohost => role.isCohost;

  bool get isEmpty => id.isEmpty;

  bool get isHost => role.isHost;

  bool get isListener => role.isListener;
}

/// What an account may do in a broadcast.
///
/// The API spells the listening role `participant`; the screens call that
/// person a listener, so the wire value is mapped here rather than renamed at
/// every call site.
enum ParticipantRole {
  host('host'),
  cohost('cohost'),
  listener('participant'),
  unknown('none');

  new(this.wire);

  final String wire;

  bool get isCohost => this == cohost;

  bool get isHost => this == host;

  bool get isInRoom => this != unknown;

  bool get isListener => this == listener;

  static ParticipantRole fromWire(String? wire) {
    for (final role in ParticipantRole.values) {
      if (role.wire == wire) return role;
    }
    return ParticipantRole.unknown;
  }
}
