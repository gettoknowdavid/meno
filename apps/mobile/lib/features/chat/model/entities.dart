import 'package:freezed_annotation/freezed_annotation.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/chat/model/reaction.dart';

part 'entities.freezed.dart';

/// Who wrote a chat message.
///
/// The API's `SenderSummary` (`apps/api/src/modules/chat/model.rs`) also carries
/// a bio and an avatar key; the chat draws a name and an avatar, so only the
/// three fields it renders are modelled.
@freezed
abstract class MessageSender with _$MessageSender {
  const factory({
    required Id id,
    required SingleLineString fullName,
    String? imageUrl,
  }) = _MessageSender;
}

/// One line of a broadcast's chat, as the API states it.
///
/// # What is not here
///
/// The pre-v2 entity carried `senderId`, `fullName` and `imageUrl` beside a
/// nullable sender, plus a `status` — each a second home for something the wire
/// already says. `sender` is always present in `ChatMessageResponse`
/// (`apps/api/src/modules/chat/dto.rs`), and sending is a fire-and-forget frame
/// rather than a promise, so there is no `sending`/`failed` state to keep.
@freezed
abstract class Message with _$Message {
  const factory({
    required Id id,
    required MultiLineString content,
    required Id broadcastId,
    required MessageSender sender,
    required DateTime createdAt,
    DateTime? updatedAt,
  }) = _Message;

  const new _();

  /// Whether the message was edited after it was written.
  bool get isEdited => updatedAt != null;

  Id get senderId => sender.id;

  SingleLineString get senderName => sender.fullName;

  String? get imageUrl => sender.imageUrl;

  /// The window the API enforces before it refuses an edit:
  /// `EDIT_WINDOW_SECONDS` in `apps/api/src/modules/chat/model.rs` (15 minutes).
  bool get isEditable =>
      DateTime.now().difference(createdAt).inMinutes < _editWindowMinutes;

  /// How long the room's UI keeps the delete action available.
  ///
  /// Deliberately shorter than the server's (unlimited) window: the design
  /// shows the option for half an hour, and hiding it sooner costs nothing.
  bool get isDeletable =>
      DateTime.now().difference(createdAt).inMinutes < _deleteWindowMinutes;

  static const int _editWindowMinutes = 15;
  static const int _deleteWindowMinutes = 30;
}

/// One reaction in a room, as `newReaction` carries it.
///
/// Deliberately not a [Message]: a reaction belongs to the broadcast rather
/// than to a line of chat — the frame has no `messageId` — and it is never
/// edited or deleted, so there is no window and nothing to update.
///
/// `content` stays the wire's string and [type] maps it, so a label this build
/// does not know is dropped by the renderer rather than refusing the frame.
/// `createdAt` is on the wire and not modelled: nothing draws when a reaction
/// arrived, only that it did, and the frame is useful the moment it lands.
final class ChatReaction {
  /// Creates one reaction.
  const new({
    required this.id,
    required this.broadcastId,
    required this.senderId,
    required this.content,
  });

  /// The reaction's id.
  final String id;

  /// The room it was sent in.
  final Id broadcastId;

  /// Who sent it.
  final Id senderId;

  /// The token or emoji the API stored.
  final String content;

  /// The reaction this build can draw, or null for one it cannot.
  ReactionType? get type => ReactionType.fromWire(content);
}
