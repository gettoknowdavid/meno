import 'package:freezed_annotation/freezed_annotation.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_core/value_objects/text_value_objects.dart';
import 'package:meno_mobile/features/broadcast/model/entities.dart';

part 'dtos.freezed.dart';
part 'dtos.g.dart';

/// `BroadcastResponse` in `apps/api/src/modules/broadcast/dto.rs`.
///
/// Carries the viewer's own standing because the API resolves it against
/// whoever asked. The `Viewer` block it also sends is not modelled: those
/// fields plus `time_remaining_seconds` are all of it.
@freezed
abstract class BroadcastDto with _$BroadcastDto {
  const factory({
    required String id,
    required String title,
    String? description,
    @JsonKey(name: 'time_zone') String? timeZone,
    @JsonKey(name: 'image_url') String? imageUrl,
    @JsonKey(name: 'image_id') String? imageId,
    @JsonKey(name: 'created_at') DateTime? createdAt,
    @JsonKey(name: 'start_time') DateTime? startTime,
    @JsonKey(name: 'end_time') DateTime? endTime,
    @JsonKey(name: 'published_at') DateTime? publishedAt,
    @JsonKey(name: 'duration_seconds') int? durationSeconds,
    @Default('inactive') String status,
    @Default('draft') String state,
    @JsonKey(name: 'participant_role') @Default('none') String participantRole,
    @JsonKey(name: 'is_joined') @Default(false) bool isJoined,
    @JsonKey(name: 'is_bookmarked') @Default(false) bool isBookmarked,
    @JsonKey(name: 'live_participants_count')
    @Default(0)
    int liveParticipantsCount,
    @JsonKey(name: 'total_participants') @Default(0) int totalParticipants,
    @JsonKey(name: 'recording_enabled') @Default(false) bool recordingEnabled,
    @JsonKey(name: 'recording_url') String? recordingUrl,
    @JsonKey(name: 'end_reason') @Default('none') String endReason,
    @JsonKey(name: 'time_remaining_seconds') int? timeRemainingSeconds,
    UserSummaryDto? creator,
    @Default(<UserSummaryDto>[]) List<UserSummaryDto> cohosts,
  }) = _BroadcastDto;

  factory fromJson(Map<String, dynamic> json) => _$BroadcastDtoFromJson(json);
}

/// `BroadcastListItem` in `apps/api/src/modules/broadcast/dto.rs`.
@freezed
abstract class BroadcastListItemDto with _$BroadcastListItemDto {
  const factory({
    required String id,
    required String title,
    String? description,
    @JsonKey(name: 'time_zone') String? timeZone,
    @JsonKey(name: 'image_url') String? imageUrl,
    @JsonKey(name: 'image_id') String? imageId,
    @Default('inactive') String status,
    @Default('draft') String state,
    @JsonKey(name: 'created_at') DateTime? createdAt,
    @JsonKey(name: 'start_time') DateTime? startTime,
    @JsonKey(name: 'end_time') DateTime? endTime,
    @JsonKey(name: 'total_participants') @Default(0) int totalParticipants,
    @JsonKey(name: 'creator_id') @Default('') String creatorId,
    @JsonKey(name: 'creator_name') @Default('') String creatorName,
    @JsonKey(name: 'creator_avatar_id') String? creatorAvatarId,
    @JsonKey(name: 'creator_avatar_url') String? creatorAvatarUrl,
  }) = _BroadcastListItemDto;

  factory fromJson(Map<String, dynamic> json) =>
      _$BroadcastListItemDtoFromJson(json);
}

/// `BroadcastSessionResponse` in `apps/api/src/modules/broadcast/dto.rs`.
///
/// What `POST /broadcasts/{id}/live` and `POST /broadcasts/{id}/join` answer
/// with: the broadcast as it now stands, plus the LiveKit token.
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
  const factory({
    required UserSummaryDto user,
    @JsonKey(name: 'cohost_count') @Default(0) int cohostCount,
  }) = _CohostDto;

  factory fromJson(Map<String, dynamic> json) => _$CohostDtoFromJson(json);
}

/// `EndBroadcastResponse` in `apps/api/src/modules/broadcast/dto.rs`.
@freezed
abstract class EndBroadcastDto with _$EndBroadcastDto {
  const factory({
    @JsonKey(name: 'broadcast_id') required String broadcastId,
    @JsonKey(name: 'broadcast_title') @Default('') String broadcastTitle,
    @JsonKey(name: 'broadcast_image_url') String? broadcastImageUrl,
    @JsonKey(name: 'creator_id') @Default('') String creatorId,
    @JsonKey(name: 'creator_name') @Default('') String creatorName,
    @JsonKey(name: 'ended_reason') @Default('normal') String endedReason,
    @JsonKey(name: 'ended_at') DateTime? endedAt,
    @JsonKey(name: 'duration_secs') @Default(0) int durationSecs,
    @JsonKey(name: 'total_participants') @Default(0) int totalParticipants,
    @JsonKey(name: 'recording_enabled') @Default(false) bool recordingEnabled,
  }) = _EndBroadcastDto;

  factory fromJson(Map<String, dynamic> json) =>
      _$EndBroadcastDtoFromJson(json);
}

/// `LeaveBroadcastResponse` in `apps/api/src/modules/broadcast/dto.rs`.
@freezed
abstract class LeaveBroadcastDto with _$LeaveBroadcastDto {
  const factory({
    @Default(false) bool success,
    @JsonKey(name: 'broadcast_id') @Default('') String broadcastId,
    @JsonKey(name: 'user_id') @Default('') String userId,
    @JsonKey(name: 'left_at') DateTime? leftAt,
  }) = _LeaveBroadcastDto;

  factory fromJson(Map<String, dynamic> json) =>
      _$LeaveBroadcastDtoFromJson(json);
}

/// `ParticipantListItem` in `apps/api/src/modules/broadcast/dto.rs`.
@freezed
abstract class ParticipantDto with _$ParticipantDto {
  const factory({
    required String id,
    @JsonKey(name: 'full_name') required String fullName,
    @JsonKey(name: 'avatar_id') String? avatarId,
    @JsonKey(name: 'avatar_url') String? avatarUrl,
    @Default('none') String role,
    @JsonKey(name: 'joined_at') DateTime? joinedAt,
    @JsonKey(name: 'is_joined') @Default(false) bool isJoined,
  }) = _ParticipantDto;

  factory fromJson(Map<String, dynamic> json) => _$ParticipantDtoFromJson(json);
}

/// `UserSummary` in `apps/api/src/modules/broadcast/model.rs`.
@freezed
abstract class UserSummaryDto with _$UserSummaryDto {
  const factory({
    required String id,
    @JsonKey(name: 'full_name') required String fullName,
    @JsonKey(name: 'avatar_id') String? avatarId,
    @JsonKey(name: 'avatar_url') String? avatarUrl,
  }) = _UserSummaryDto;

  factory fromJson(Map<String, dynamic> json) => _$UserSummaryDtoFromJson(json);
}

/// The domain view of a [BroadcastDto].
extension BroadcastDtoX on BroadcastDto {
  Broadcast get toDomain => Broadcast(
    id: Id.fromString(id),
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
    id: Id.fromString(id),
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
    creatorId: Id.fromString(creatorId),
    creatorName: FullName(creatorName),
    creatorAvatarId: creatorAvatarId,
    creatorAvatar: ImageInput.fromUrl(creatorAvatarUrl),
  );
}

/// The domain view of a [BroadcastSessionDto].
extension BroadcastSessionDtoX on BroadcastSessionDto {
  /// This session as the app models it, under [userId].
  BroadcastSession toDomain(String userId) => BroadcastSession(
    userId: Id.fromString(userId),
    broadcast: broadcast.toDomain,
    token: token,
    canPublish: broadcast.toDomain.canPublish,
    timestamp: DateTime.now(),
  );
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
    endReason: endReason.wire,
    timeRemainingSeconds: timeRemainingSeconds,
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
    broadcastId: Id.fromString(broadcastId),
    title: SingleLineString(broadcastTitle),
    creatorId: Id.fromString(creatorId),
    creatorName: FullName(creatorName),
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
    id: Id.fromString(id),
    fullName: FullName(fullName),
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
    id: Id.fromString(id),
    fullName: FullName(fullName),
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
