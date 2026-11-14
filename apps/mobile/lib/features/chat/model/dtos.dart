import 'package:freezed_annotation/freezed_annotation.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/chat/model/entities.dart';

part 'dtos.freezed.dart';
part 'dtos.g.dart';

/// The API's `SenderSummary`, read once.
///
/// Keys are the wire's own camelCase spelling — `fullName`, `avatarUrl` — which
/// `apps/api/src/modules/chat/dto.rs` emits for both the HTTP page and the
/// socket frames, so one decoder serves both.
@freezed
abstract class MessageSenderDto with _$MessageSenderDto {
  const factory({
    required String id,
    required String fullName,
    String? avatarUrl,
  }) = _MessageSenderDto;

  factory fromJson(Map<String, dynamic> json) =>
      _$MessageSenderDtoFromJson(json);

  const new _();

  MessageSender get toDomain => MessageSender(
    id: Id.fromString(id),
    fullName: SingleLineString(fullName),
    imageUrl: avatarUrl,
  );
}

/// One `ChatMessageResponse`, read once.
///
/// The same body arrives on the HTTP page and on the `newMessage` and
/// `editedMessage` frames, and `tests/wire_dates.rs` keeps its dates RFC 3339
/// for both, so a message cannot be read two different ways.
@freezed
abstract class MessageDto with _$MessageDto {
  const factory({
    required String id,
    required String content,
    required String broadcastId,
    required MessageSenderDto sender,
    required DateTime createdAt,
    DateTime? updatedAt,
  }) = _MessageDto;

  factory fromJson(Map<String, dynamic> json) =>
      _$MessageDtoFromJson(json);

  const new _();

  Message get toDomain => Message(
    id: Id.fromString(id),
    content: MultiLineString(content),
    broadcastId: Id.fromString(broadcastId),
    sender: sender.toDomain,
    createdAt: createdAt,
    updatedAt: updatedAt,
  );
}

/// What the client sends, as `WsSendMessage` names it.
///
/// There is no `senderId` field, by design: the API fills the identity in from
/// the authenticated socket, so a client cannot post as somebody else by adding
/// one (`apps/api/src/infrastructure/ws/dto.rs`).
@freezed
abstract class NewMessageDto with _$NewMessageDto {
  const factory({
    required String broadcastId,
    required String content,
  }) = _NewMessageDto;

  factory fromJson(Map<String, dynamic> json) =>
      _$NewMessageDtoFromJson(json);
}

/// What the client sends, as `WsEditMessage` names it.
@freezed
abstract class EditMessageDto with _$EditMessageDto {
  const factory({
    required String broadcastId,
    required String messageId,
    required String content,
  }) = _EditMessageDto;

  factory fromJson(Map<String, dynamic> json) =>
      _$EditMessageDtoFromJson(json);
}

/// What the client sends, as `WsDeleteMessage` names it.
@freezed
abstract class DeleteMessageDto with _$DeleteMessageDto {
  const factory({
    required String broadcastId,
    required String messageId,
  }) = _DeleteMessageDto;

  factory fromJson(Map<String, dynamic> json) =>
      _$DeleteMessageDtoFromJson(json);
}
