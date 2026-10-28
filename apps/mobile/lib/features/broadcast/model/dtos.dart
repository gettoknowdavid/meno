import 'package:freezed_annotation/freezed_annotation.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/model/entities.dart';

part 'dtos.freezed.dart';
part 'dtos.g.dart';

/// `BroadcastResponse` in `apps/api/src/modules/broadcast/dto.rs`.
///
/// Carries the viewer's own standing because the API resolves it against
/// whoever asked. The `Viewer` block it also sends is not modelled: those
/// fields plus `time_remaining_seconds` are all of it.
///
/// Wire names are camelCase because the backend serialises every response
/// type with `#[serde(rename_all = "camelCase")]` (§6.2). The fields the
/// backend always sends — `status`, `state`, `participantRole`, `isJoined`,
/// `isBookmarked`, the counts, `recordingEnabled` — are `required`, not
/// defaulted, so a response missing one fails to parse rather than silently
/// reporting `false`.
@freezed
abstract class BroadcastDto with _$BroadcastDto {
  const factory({
    required String id,
    required String title,
    required String status,
    required String state,
    required String participantRole,
    required bool isJoined,
    required bool isBookmarked,
    required int liveParticipantsCount,
    required int totalParticipants,
    required bool recordingEnabled,
    required List<UserSummaryDto> cohosts,
    String? description,
    String? timeZone,
    String? imageUrl,
    String? imageId,
    DateTime? createdAt,
    DateTime? startTime,
    DateTime? endTime,
    DateTime? publishedAt,
    int? durationSeconds,
    String? recordingUrl,

    /// `null` while the broadcast has not ended: a live broadcast must not
    /// carry a reason it ended (D-20, Tier 1.2). The domain maps `null` to
    /// `EndReason.none`.
    String? endReason,
    int? timeRemainingSeconds,
    String? creatorId,
    UserSummaryDto? creator,
  }) = _BroadcastDto;

  factory fromJson(Map<String, dynamic> json) => _$BroadcastDtoFromJson(json);
}

/// `BroadcastListItem` in `apps/api/src/modules/broadcast/dto.rs`.
@freezed
abstract class BroadcastListItemDto with _$BroadcastListItemDto {
  const factory({
    required String id,
    required String title,
    required String status,
    required String state,
    required int totalParticipants,
    required String creatorId,
    required String creatorName,
    String? description,
    String? timeZone,
    String? imageUrl,
    String? imageId,
    DateTime? createdAt,
    DateTime? startTime,
    DateTime? endTime,
    String? creatorAvatarId,
    String? creatorAvatarUrl,
  }) = _BroadcastListItemDto;

  factory fromJson(Map<String, dynamic> json) =>
      _$BroadcastListItemDtoFromJson(json);
}

/// `BroadcastSessionResponse` in `apps/api/src/modules/broadcast/dto.rs`.
///
/// What `POST /broadcasts/{id}/live`, `/join`, `/resume` and `/token` answer
/// with: the broadcast as it now stands, plus the LiveKit token. One envelope
/// for every way into a room.
@freezed
abstract class BroadcastSessionDto with _$BroadcastSessionDto {
  const factory({required BroadcastDto broadcast, required String token}) =
      _BroadcastSessionDto;

  factory fromJson(Map<String, dynamic> json) =>
      _$BroadcastSessionDtoFromJson(json);
}

/// `CohostResponse` in `apps/api/src/modules/broadcast/dto.rs`.
@freezed
abstract class CohostDto with _$CohostDto {
  const factory({required UserSummaryDto user, required int cohostCount}) =
      _CohostDto;

  factory fromJson(Map<String, dynamic> json) => _$CohostDtoFromJson(json);
}

/// Parses a timestamp the socket sent, or fails.
///
/// RFC3339 only: the API's contract is a text instant (`2026-10-28T09:15:00Z`),
/// and anything else is a payload this build cannot act on. That matters
/// because the alternative is silent — `DateTime.parse` accepts a bare date,
/// and a `tryParse`/`catch` that fell back to `DateTime.now()` would turn a
/// serialisation bug into a plausible-looking time (the H10 shape). An
/// `OffsetDateTime` serialised as an array — which is what serde does without
/// its RFC3339 adapter — therefore fails here, visibly, in a test fixture.
DateTime parseRfc3339(Object? value) {
  if (value is! String) {
    throw FormatException('Expected an RFC3339 timestamp string', value);
  }
  if (!_rfc3339.hasMatch(value)) {
    throw FormatException('Not an RFC3339 timestamp', value);
  }
  return DateTime.parse(value);
}

/// A full RFC3339 instant: date, time, optional fraction, and an offset or `Z`.
final _rfc3339 = RegExp(
  r'^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})$',
);

/// The socket's `homeInvalidated` frame.
///
/// The API's `EndBroadcastResponse` is the richer type: it carries the ending
/// summary. The socket's `homeInvalidated` is a bare timestamp the client uses
/// as a nudge, and the client decodes it through a DTO of its own so the parse
/// path is explicit rather than conditional — [parseRfc3339], not a permissive
/// `DateTime.parse` and never a `tryParse` that defaults to now.
@freezed
abstract class HomeInvalidatedDto with _$HomeInvalidatedDto {
  const factory({
    @JsonKey(fromJson: parseRfc3339) required DateTime timestamp,
  }) = _HomeInvalidatedDto;

  factory fromJson(Map<String, dynamic> json) =>
      _$HomeInvalidatedDtoFromJson(json);
}

/// `EndBroadcastResponse` in `apps/api/src/modules/broadcast/dto.rs`.
@freezed
abstract class EndBroadcastDto with _$EndBroadcastDto {
  const factory({
    required String broadcastId,
    required String broadcastTitle,
    required String creatorId,
    required String creatorName,
    required String endedReason,
    required int durationSecs,
    required int totalParticipants,
    required bool recordingEnabled,
    String? broadcastImageUrl,
    DateTime? endedAt,
  }) = _EndBroadcastDto;

  factory fromJson(Map<String, dynamic> json) =>
      _$EndBroadcastDtoFromJson(json);
}

/// `LeaveBroadcastResponse` in `apps/api/src/modules/broadcast/dto.rs`.
@freezed
abstract class LeaveBroadcastDto with _$LeaveBroadcastDto {
  const factory({
    required bool success,
    required String broadcastId,
    required String userId,
    DateTime? leftAt,
  }) = _LeaveBroadcastDto;

  factory fromJson(Map<String, dynamic> json) =>
      _$LeaveBroadcastDtoFromJson(json);
}

/// `ParticipantListItem` in `apps/api/src/modules/broadcast/dto.rs`.
@freezed
abstract class ParticipantDto with _$ParticipantDto {
  const factory({
    required String id,
    required String fullName,
    required String role,
    required bool isJoined,
    String? avatarId,
    String? avatarUrl,
    DateTime? joinedAt,
  }) = _ParticipantDto;

  factory fromJson(Map<String, dynamic> json) => _$ParticipantDtoFromJson(json);
}

/// `UserSummary` in `apps/api/src/modules/broadcast/model.rs`.
@freezed
abstract class UserSummaryDto with _$UserSummaryDto {
  const factory({
    required String id,
    required String fullName,
    String? avatarId,
    String? avatarUrl,
  }) = _UserSummaryDto;

  factory fromJson(Map<String, dynamic> json) => _$UserSummaryDtoFromJson(json);
}

/// An id the wire may leave out: the empty string is no id at all.
Id _wireId(String raw) => raw.isEmpty ? Id.empty : Id.fromWire(raw);

/// The domain view of a [BroadcastDto].
extension BroadcastDtoX on BroadcastDto {
  Broadcast get toDomain => Broadcast(
    id: Id.fromWire(id),
    title: SingleLineString(title),
    description: description == null ? null : MultiLineString(description!),
    timeZone: timeZone,
    image: ImageInput.fromUrl(imageUrl),
    imageId: imageId,
    createdAt: createdAt,
    startTime: startTime,
    endTime: endTime,
    publishedAt: publishedAt,
    durationSeconds: durationSeconds,
    status: BroadcastStatus.fromWire(status),
    state: BroadcastState.fromWire(state),
    participantRole: ParticipantRole.fromWire(participantRole),
    isJoined: isJoined,
    isBookmarked: isBookmarked,
    liveParticipantsCount: liveParticipantsCount,
    totalParticipants: totalParticipants,
    recordingEnabled: recordingEnabled,
    recordingUrl: recordingUrl,
    endReason: EndReason.fromWire(endReason),
    timeRemainingSeconds: timeRemainingSeconds,
    creator: creator?.toDomain,
    cohosts: [for (final cohost in cohosts) cohost.toDomain],
  );
}

/// The domain view of a [BroadcastListItemDto].
extension BroadcastListItemDtoX on BroadcastListItemDto {
  BroadcastSummary get toDomain => BroadcastSummary(
    id: Id.fromWire(id),
    title: SingleLineString(title),
    description: description == null ? null : MultiLineString(description!),
    timeZone: timeZone,
    image: ImageInput.fromUrl(imageUrl),
    imageId: imageId,
    status: BroadcastStatus.fromWire(status),
    state: BroadcastState.fromWire(state),
    createdAt: createdAt,
    startTime: startTime,
    endTime: endTime,
    totalParticipants: totalParticipants,
    creatorId: _wireId(creatorId),
    creatorName: SingleLineString(creatorName),
    creatorAvatarId: creatorAvatarId,
    creatorAvatar: ImageInput.fromUrl(creatorAvatarUrl),
  );
}

/// The domain view of a [BroadcastSessionDto].
extension BroadcastSessionDtoX on BroadcastSessionDto {
  /// This session as the app models it, under [userId].
  BroadcastSession toDomain(Id userId) {
    final domain = broadcast.toDomain;
    return BroadcastSession(
      userId: userId,
      broadcast: domain,
      token: token,
      canPublish: domain.canPublish,
      timestamp: DateTime.now(),
    );
  }
}

/// The wire view of a [Broadcast], for the cache.
extension BroadcastX on Broadcast {
  BroadcastDto get toDto => BroadcastDto(
    id: id.getOrCrash,
    title: title.getOrCrash,
    description: description?.getOrNull,
    timeZone: timeZone,
    imageUrl: image?.toUrlOrFilePathOrNull,
    imageId: imageId,
    createdAt: createdAt,
    startTime: startTime,
    endTime: endTime,
    publishedAt: publishedAt,
    durationSeconds: durationSeconds,
    status: status.wire,
    state: state.wire,
    participantRole: participantRole.wire,
    isJoined: isJoined,
    isBookmarked: isBookmarked,
    liveParticipantsCount: liveParticipantsCount,
    totalParticipants: totalParticipants,
    recordingEnabled: recordingEnabled,
    recordingUrl: recordingUrl,
    endReason: endReason == EndReason.none ? null : endReason.wire,
    timeRemainingSeconds: timeRemainingSeconds,
    // Absent for a creatorless draft; `null` rather than an empty id, so the
    // cached copy serialises exactly what the API would have sent.
    creatorId: creator == null || creator!.id.isEmpty
        ? null
        : creator!.id.getOrCrash,
    creator: creator?.toDto,
    cohosts: [for (final cohost in cohosts) cohost.toDto],
  );
}

/// The domain view of a [CohostDto].
extension CohostDtoX on CohostDto {
  /// The account that was added, and how many cohosts the broadcast now has.
  ({Participant user, int count}) get toDomain =>
      (user: user.toDomain, count: cohostCount);
}

/// The domain view of an [EndBroadcastDto].
extension EndBroadcastDtoX on EndBroadcastDto {
  EndedBroadcast get toDomain => EndedBroadcast(
    broadcastId: Id.fromWire(broadcastId),
    title: SingleLineString(broadcastTitle),
    creatorId: _wireId(creatorId),
    creatorName: SingleLineString(creatorName),
    endedAt: endedAt,
    image: ImageInput.fromUrl(broadcastImageUrl),
    reason: EndReason.fromWire(endedReason),
    duration: Duration(seconds: durationSecs),
    totalParticipants: totalParticipants,
    recordingEnabled: recordingEnabled,
  );
}

/// The domain view of a [ParticipantDto].
extension ParticipantDtoX on ParticipantDto {
  Participant get toDomain => Participant(
    id: Id.fromWire(id),
    fullName: SingleLineString(fullName),
    avatarId: avatarId,
    avatar: ImageInput.fromUrl(avatarUrl),
    role: ParticipantRole.fromWire(role),
    joinedAt: joinedAt,
    isJoined: isJoined,
  );
}

/// The wire view of a [Participant].
extension ParticipantX on Participant {
  UserSummaryDto get toDto => UserSummaryDto(
    id: id.getOrCrash,
    fullName: fullName.getOrCrash,
    avatarId: avatarId,
    avatarUrl: avatar?.toUrlOrFilePathOrNull,
  );
}

/// The domain view of a [UserSummaryDto].
extension UserSummaryDtoX on UserSummaryDto {
  Participant get toDomain => Participant(
    id: Id.fromWire(id),
    fullName: SingleLineString(fullName),
    avatarId: avatarId,
    avatar: ImageInput.fromUrl(avatarUrl),
  );

  /// This account as the API writes it.
  UserSummaryDto get toDto => UserSummaryDto(
    id: id,
    fullName: fullName,
    avatarId: avatarId,
    avatarUrl: avatarUrl,
  );
}
