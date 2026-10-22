import 'package:freezed_annotation/freezed_annotation.dart';
import 'package:meno_mobile/_core/_core.dart';

part 'entities.freezed.dart';

// final fakeBroadcast = Broadcast(
//   id: Id.fromString('2a84d446-555d-4c21-acd0-a13045f23e12'),
//   title: SingleLineString('Prayer & Testimony Night'),
//   description: MultiLineString(
//     'Open mic for prayers and shared testimonies.',
//   ),
//   timeZone: 'Europe/London',
//   imageId: 'img-123',
//   status: .active,
//   state: .live,
//   isJoined: true,
//   isBookmarked: true,
//   liveParticipantsCount: 18,
//   totalParticipants: 42,
//   recordingEnabled: true,
//   timeRemainingSeconds: 1480,
//   creator: Participant(
//     id: Id.fromString('a2ac10be-46ab-48ff-bbed-21a4fc0b87c7'),
//     fullName: SingleLineString('David Michael'),
//     bio: MultiLineString(
//       '''
// Blessed of the Most High God, the possessor of heaven and earth! So am I
// and I give thanks!''',
//     ),
//   ),
//   // cohosts: [
//   //   Participant(
//   //     id: Id.fromString('cec40b96-e28b-4fb2-af85-6329ee39474c'),
//   //     fullName: SingleLineString('Priya Nair'),
//   //     bio: MultiLineString('Hosting with Marcus tonight.'),
//   //   ),
//   // ],
// );

/// Sample broadcasts in different states for cards, skeletons, and empty views.
final List<Broadcast> fakeBroadcastList = [
  Broadcast(
    id: Id.fromString('c1c1c1c1-0001-4c1c-8c1c-1c1c1c1c1c1c'),
    title: SingleLineString('Sunday Worship Live'),
    description: MultiLineString(
      'A weekly time of worship, prayer, and the word.',
    ),
    timeZone: 'America/New_York',
    createdAt: DateTime(2024, 1, 10),
    startTime: DateTime.now().add(const Duration(hours: 2)),
    endTime: DateTime.now().add(const Duration(hours: 3)),
    status: .active,
    state: .scheduled,
    recordingEnabled: true,
    creator: Participant(
      id: Id.fromString('a1a1a1a1-0001-4a1a-8a1a-1a1a1a1a1a1a'),
      fullName: SingleLineString('Elena Vasquez'),
    ),
  ),
  Broadcast(
    id: Id.fromString('c1c1c1c1-0002-4c1c-8c1c-2c1c1c1c1c1c'),
    title: SingleLineString('Prayer & Testimony Night'),
    description: MultiLineString(
      'Open mic for prayers and shared testimonies.',
    ),
    timeZone: 'Europe/London',
    imageId: 'img-123',
    status: .active,
    state: .live,
    isJoined: true,
    isBookmarked: true,
    liveParticipantsCount: 18,
    totalParticipants: 42,
    recordingEnabled: true,
    timeRemainingSeconds: 1480,
    creator: Participant(
      id: Id.fromString('a1a1a1a1-0002-4a1a-8a1a-2a1a1a1a1a1a'),
      fullName: SingleLineString('Marcus Reed'),
    ),
    cohosts: [
      Participant(
        id: Id.fromString('a1a1a1a1-0003-4a1a-8a1a-3a1a1a1a1a1a'),
        fullName: SingleLineString('Priya Nair'),
        bio: MultiLineString('Hosting with Marcus tonight.'),
      ),
    ],
  ),
  Broadcast(
    id: Id.fromString('c1c1c1c1-0003-4c1c-8c1c-3c1c1c1c1c1c'),
    title: SingleLineString('Morning Bible Study'),
    status: .ended,
    state: .ended,
    isBookmarked: true,
    endReason: EndReason.normal,
  ),
  Broadcast(
    id: Id.fromString('c1c1c1c1-0004-4c1c-8c1c-4c1c1c1c1c1c'),
    title: SingleLineString('Guest Speaker: Faith in Action'),
    description: MultiLineString(
      'A special session with a guest speaker on serving the community.',
    ),
    timeZone: 'Asia/Tokyo',
    status: .active,
    state: .scheduled,
    startTime: DateTime.now().add(const Duration(days: 3, hours: 1)),
    endTime: DateTime.now().add(const Duration(days: 3, hours: 2)),
    creator: Participant(
      id: Id.fromString('a1a1a1a1-0004-4a1a-8a1a-4a1a1a1a1a1a'),
      fullName: SingleLineString('Jordan Smith'),
    ),
  ),
  Broadcast(
    id: Id.fromString('c1c1c1c1-0005-4c1c-8c1c-5c1c1c1c1c1c'),
    title: SingleLineString('Youth Night'),
  ),
];

/// Sample favorites for the saved-broadcasts list and empty-state tests.
final List<FavouriteBroadcast> fakeFavouriteBroadcastList = [
  FavouriteBroadcast(
    broadcastId: Id.fromString('c1c1c1c1-0002-4c1c-8c1c-2c1c1c1c1c1c'),
    ownerId: Id.fromString('a1a1a1a1-0001-4a1a-8a1a-1a1a1a1a1a1a'),
    title: SingleLineString('Prayer & Testimony Night'),
    creatorName: SingleLineString('Marcus Reed'),
  ),
  FavouriteBroadcast(
    broadcastId: Id.fromString('c1c1c1c1-0001-4c1c-8c1c-1c1c1c1c1c1c'),
    ownerId: Id.fromString('a1a1a1a1-0001-4a1a-8a1a-1a1a1a1a1a1a'),
    title: SingleLineString('Sunday Worship Live'),
    creatorName: SingleLineString('Elena Vasquez'),
    savedAt: DateTime(2026, 4, 3),
  ),
  FavouriteBroadcast(
    broadcastId: Id.fromString('c1c1c1c1-0003-4c1c-8c1c-3c1c1c1c1c1c'),
    ownerId: Id.fromString('a1a1a1a1-0001-4a1a-8a1a-1a1a1a1a1a1a'),
    title: SingleLineString('Morning Bible Study'),
    savedAt: DateTime(2026, 6, 20),
  ),
];

/// Sample participants, including hosts, cohost, listeners, and
/// empty-state cases.
final List<Participant> fakeParticipantList = [
  Participant(
    id: Id.fromString('a1a1a1a1-0001-4a1a-8a1a-1a1a1a1a1a1a'),
    fullName: SingleLineString('Elena Vasquez'),
    role: ParticipantRole.host,
  ),
  Participant(
    id: Id.fromString('a1a1a1a1-0003-4a1a-8a1a-3a1a1a1a1a1a'),
    fullName: SingleLineString('Priya Nair'),
    bio: MultiLineString('Co-hosting tonight.'),
    avatarId: 'av-456',
    role: ParticipantRole.cohost,
  ),
  Participant(
    id: Id.fromString('a1a1a1a1-0005-4a1a-8a1a-5a1a1a1a1a1a'),
    fullName: SingleLineString('Sam Carter'),
    role: ParticipantRole.listener,
  ),
  Participant(
    id: Id.fromString('a1a1a1a1-0006-4a1a-8a1a-6a1a1a1a1a1a'),
    fullName: SingleLineString('Taylor Kim'),
  ),
  Participant.empty,
];

/// A broadcast as `GET /broadcasts/{id}` answers with.
///
/// The shape is the API's `BroadcastResponse`. The viewer's own standing —
/// [participantRole], [isJoined], [isBookmarked] — is kept here rather than in
/// a second view object, because every screen that draws one needs it.
@freezed
abstract class Broadcast with _$Broadcast {
  const factory({
    required Id id,
    required SingleLineString title,
    MultiLineString? description,
    String? timeZone,
    ImageInput? image,
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
  static const Broadcast empty = Broadcast(id: .empty, title: .empty);

  /// Whether the viewer may publish into the room.
  bool get canPublish => participantRole.isHost || participantRole.isCohost;

  /// How long an ended broadcast ran, when the API stated it.
  Duration? get duration =>
      durationSeconds == null ? null : Duration(seconds: durationSeconds!);

  bool get hasEnded => state.hasEnded || endReason != EndReason.none;

  /// The account that owns the broadcast.
  Id get hostId => creator?.id ?? .empty;

  /// The host's avatar, when one was uploaded.
  ImageInput? get hostImageUrl => creator?.avatar;

  /// The host's name, for a card with no room for a full participant.
  SingleLineString get hostName => creator?.fullName ?? .empty;

  bool get isDraft => state == .draft;

  bool get isEmpty => id.isEmpty;

  /// Whether the broadcast is on air, a reconnecting host included.
  bool get isLive => state.onAir;

  bool get isNotEmpty => id.isValid;

  bool get isScheduled => state == .scheduled;

  /// How much of a scheduled broadcast is left, when the API stated it.
  Duration? get timeRemaining => timeRemainingSeconds == null
      ? null
      : Duration(seconds: timeRemainingSeconds!);

  bool isHostedBy(Id userId) => hostId == userId;
}

/// Which step of creating a broadcast a draft got to.
enum BroadcastCreationStep { none, created, started, saved }

/// A broadcast being written, before it exists anywhere but the device.
@freezed
abstract class BroadcastDraft with _$BroadcastDraft {
  const factory({
    required Id id,
    @Default(Id.empty) Id ownerId,
    @Default('') String title,
    String? description,
    @Default(false) bool record,
    @Default(<Id>[]) List<Id> cohosts,
    ImageInput? imagePath,
    String? imageId,
    Id? createdBroadcastId,
    @Default(BroadcastCreationStep.none) BroadcastCreationStep creationStep,
    DateTime? lastModified,
  }) = _BroadcastDraft;

  const new _();

  /// Whether a broadcast was created on the server that this draft still owns.
  bool get hasPartialCreation =>
      creationStep != BroadcastCreationStep.none &&
      (createdBroadcastId?.isValid ?? false);

  /// Whether the draft has been written to at all.
  bool get isEmpty =>
      title.trim().isEmpty && (description?.trim().isEmpty ?? true);
}

/// Which way a broadcast list is ordered. Mirrors `meno_core::Order`.
enum BroadcastOrder {
  newest('desc'),
  oldest('asc');

  new(this.wire);

  final String wire;
}

/// The session a client joins a live broadcast with.
///
/// `POST /broadcasts/{id}/live` and `POST /broadcasts/{id}/join` answer with the
/// broadcast and the LiveKit token for it. The account it belongs to travels
/// with it, so live code never reaches for the auth manager mid-broadcast.
@freezed
abstract class BroadcastSession with _$BroadcastSession {
  const factory({
    required Id userId,
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
    return .draft;
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
    return .inactive;
  }
}

/// A broadcast as a list answers with it.
///
/// The API's `BroadcastListItem`: enough to draw a card, and deliberately
/// without the viewer's own standing, which only the single-broadcast endpoint
/// resolves. The creator arrives flattened rather than as a nested object.
@freezed
abstract class BroadcastSummary with _$BroadcastSummary {
  const factory({
    required Id id,
    required SingleLineString title,
    MultiLineString? description,
    String? timeZone,
    ImageInput? image,
    String? imageId,
    @Default(BroadcastStatus.inactive) BroadcastStatus status,
    @Default(BroadcastState.draft) BroadcastState state,
    DateTime? createdAt,
    DateTime? startTime,
    DateTime? endTime,
    @Default(0) int totalParticipants,
    @Default(Id.empty) Id creatorId,
    @Default(SingleLineString.empty) SingleLineString creatorName,
    String? creatorAvatarId,
    ImageInput? creatorAvatar,
  }) = _BroadcastSummary;

  const new _();

  static const BroadcastSummary empty = BroadcastSummary(
    id: .empty,
    title: .empty,
  );

  bool get hasEnded => state.hasEnded;

  bool get isEmpty => id.isEmpty;

  bool get isLive => state.onAir;

  bool get isScheduled => state == .scheduled;

  bool isHostedBy(Id userId) => creatorId == userId;

  /// This summary as the richer type, for code that takes a [Broadcast].
  Broadcast toBroadcast() => Broadcast(
    id: id,
    title: title,
    description: description,
    timeZone: timeZone,
    image: image,
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
            avatar: creatorAvatar,
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
    required Id broadcastId,
    required SingleLineString title,
    @Default(Id.empty) Id creatorId,
    @Default(SingleLineString.empty) SingleLineString creatorName,
    DateTime? endedAt,
    ImageInput? image,
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
    image: image,
    endTime: endedAt,
    endReason: reason,
    status: .ended,
    state: .ended,
    durationSeconds: duration.inSeconds,
    totalParticipants: totalParticipants,
    recordingEnabled: recordingEnabled,
    creator: creatorId.isEmpty
        ? null
        : Participant(id: creatorId, fullName: creatorName),
  );

  bool get isEmpty => broadcastId.isEmpty;

  bool isHostedBy(Id userId) => creatorId == userId;
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
    required Id broadcastId,
    required Id ownerId,
    @Default(SingleLineString.empty) SingleLineString title,
    @Default(SingleLineString.empty) SingleLineString creatorName,
    ImageInput? image,
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
    required Id id,
    required SingleLineString fullName,
    String? avatarId,
    ImageInput? avatar,
    MultiLineString? bio,
    @Default(ParticipantRole.unknown) ParticipantRole role,
    @Default(false) bool isJoined,
    DateTime? joinedAt,
  }) = _Participant;

  const new _();

  static const Participant empty = Participant(id: .empty, fullName: .empty);

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

/// The broadcast one live scope is open for.
@freezed
abstract class LiveSession with _$LiveSession {
  const factory({required Id broadcastId, required Id userId}) = _LiveSession;
}
