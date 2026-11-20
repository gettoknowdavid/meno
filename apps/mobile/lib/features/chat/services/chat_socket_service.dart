import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/model/socket.dart';
import 'package:meno_mobile/_shared/services/socket_client.dart';
import 'package:meno_mobile/features/chat/model/_model.dart';

/// A message that was deleted, as `deletedMessage` states it.
///
/// The frame carries an id pair and no body — `WsPayload::deleted_message` in
/// `apps/api/src/infrastructure/ws/dto.rs` — so this is all a client can know
/// about the deletion without refetching.
final class DeletedMessageSignal {
  const new({required this.messageId, required this.broadcastId});

  /// Reads the frame's body, or null when it is not one.
  static DeletedMessageSignal? tryParse(dynamic data) {
    if (data is! Map<String, dynamic>) return null;
    final messageId = data['messageId'];
    final broadcastId = data['broadcastId'];
    if (messageId is! String || broadcastId is! String) return null;
    return DeletedMessageSignal(
      messageId: messageId,
      broadcastId: Id.fromString(broadcastId),
    );
  }

  /// The message that is gone.
  final String messageId;

  /// The broadcast it belonged to.
  final Id broadcastId;
}

/// The chat events a room's screens need, from one place.
final class ChatSocketService
    implements ConnectionAdapter, Disposable, ChatSocketChannel {
  new({required this._client, required this._logger});

  final SocketClient _client;
  final MenoLogger _logger;

  final _newMessage = StreamController<Message>.broadcast();
  final _editedMessage = StreamController<Message>.broadcast();
  final _deletedMessage = StreamController<DeletedMessageSignal>.broadcast();
  final _reaction = StreamController<ChatReaction>.broadcast();

  List<SocketSubscription> _handles = const [];
  bool _installed = false;

  @override
  Stream<Message> get onNewMessage => _newMessage.stream;

  @override
  Stream<Message> get onEditedMessage => _editedMessage.stream;

  @override
  Stream<DeletedMessageSignal> get onDeletedMessage => _deletedMessage.stream;

  @override
  Stream<ChatReaction> get onReaction => _reaction.stream;

  @override
  Stream<void> get onReconnected => _client.onReconnected;

  /// Attaches one handler per server→client event. Idempotent.
  @override
  void install() {
    if (_installed) return;
    _installed = true;

    _handles = [
      _client.on(
        SocketEvent.newMessage,
        (data) => _dispatchMessage(data, _newMessage, 'newMessage'),
      ),
      _client.on(
        SocketEvent.editedMessage,
        (data) => _dispatchMessage(data, _editedMessage, 'editedMessage'),
      ),
      _client.on(SocketEvent.deletedMessage, _dispatchDeleted),
      _client.on(SocketEvent.newReaction, _dispatchReaction),
    ];
  }

  /// Removes the handlers. Idempotent.
  @override
  void detach() {
    if (!_installed) return;
    _installed = false;

    final handles = _handles;
    _handles = const [];
    for (final handle in handles) {
      handle.cancel();
    }
  }

  @override
  void sendMessage({required Id broadcastId, required String content}) {
    _client.send(
      SocketEvent.sendChatMessage,
      NewMessageDto(
        broadcastId: broadcastId.getOrCrash,
        content: content,
      ).toJson(),
    );
  }

  @override
  void editMessage({
    required Id broadcastId,
    required Id messageId,
    required String content,
  }) {
    _client.send(
      SocketEvent.editChatMessage,
      EditMessageDto(
        broadcastId: broadcastId.getOrCrash,
        messageId: messageId.getOrCrash,
        content: content,
      ).toJson(),
    );
  }

  @override
  void deleteMessage({required Id broadcastId, required Id messageId}) {
    _client.send(
      SocketEvent.deleteChatMessage,
      DeleteMessageDto(
        broadcastId: broadcastId.getOrCrash,
        messageId: messageId.getOrCrash,
      ).toJson(),
    );
  }

  @override
  void sendReaction({required Id broadcastId, required ReactionType type}) {
    _client.send(
      SocketEvent.sendReaction,
      SendReactionDto(
        broadcastId: broadcastId.getOrCrash,
        content: type.wire,
      ).toJson(),
    );
  }

  /// Decodes one message frame, dropping a malformed one instead of throwing.
  ///
  /// The handler runs inside the socket's dispatch loop, where a throw is
  /// logged and the frame lost anyway; decoding here keeps the failure one
  /// sanitised line rather than a stack from the transport.
  void _dispatchMessage(
    dynamic data,
    StreamController<Message> sink,
    String event,
  ) {
    if (data is! Map<String, dynamic>) {
      _logger.w(
        'ChatSocketService: dropped a $event frame that is not an object',
        tag: MenoLogTag.network.label,
      );
      return;
    }

    try {
      sink.add(MessageDto.fromJson(data).toDomain);
    } on Object catch (error, stackTrace) {
      _logger.w(
        'ChatSocketService: dropped a $event frame that could not be read',
        tag: MenoLogTag.network.label,
        error: error,
        stackTrace: stackTrace,
      );
    }
  }

  /// Decodes one `newReaction` frame, dropping a malformed one.
  ///
  /// The API serialises `ChatReactionResponse` with camelCase keys and an
  /// RFC3339 `createdAt`; the date is not read, so a frame is a reaction the
  /// moment its four ids and label are there.
  void _dispatchReaction(dynamic data) {
    if (data is! Map<String, dynamic>) {
      _logger.w(
        'ChatSocketService: dropped a newReaction frame that is not an object',
        tag: MenoLogTag.network.label,
      );
      return;
    }

    try {
      _reaction.add(parseChatReaction(data));
    } on Object catch (error, stackTrace) {
      _logger.w(
        'ChatSocketService: dropped a newReaction frame that could not be read',
        tag: MenoLogTag.network.label,
        error: error,
        stackTrace: stackTrace,
      );
    }
  }

  void _dispatchDeleted(dynamic data) {
    final signal = DeletedMessageSignal.tryParse(data);
    if (signal == null) {
      _logger.w(
        'ChatSocketService: dropped a deletedMessage frame without ids',
        tag: MenoLogTag.network.label,
      );
      return;
    }
    _deletedMessage.add(signal);
  }

  @override
  FutureOr<dynamic> onDispose() async {
    detach();
    await _newMessage.close();
    await _editedMessage.close();
    await _deletedMessage.close();
    await _reaction.close();
  }
}

/// What a room's chat managers need from the socket.
///
/// A stream per event and the three frames a client may send — no socket, no
/// connection, no event registry. [ChatSocketService] implements it in
/// production and a manager test implements it with a controller it can push
/// through.
abstract interface class ChatSocketChannel {
  /// A message somebody posted.
  Stream<Message> get onNewMessage;

  /// A message that was edited; its body is the replacement.
  Stream<Message> get onEditedMessage;

  /// A message that was deleted.
  Stream<DeletedMessageSignal> get onDeletedMessage;

  /// A reaction somebody in a room sent.
  ///
  /// Delivered for every room this account's socket carries, so a listener
  /// filters by `broadcastId` — the room's manager does.
  Stream<ChatReaction> get onReaction;

  /// The socket reconnected, so frames may have been missed.
  Stream<void> get onReconnected;

  /// Posts [content] in [broadcastId].
  void sendMessage({required Id broadcastId, required String content});

  /// Replaces [messageId]'s body with [content].
  void editMessage({
    required Id broadcastId,
    required Id messageId,
    required String content,
  });

  /// Deletes [messageId].
  void deleteMessage({required Id broadcastId, required Id messageId});

  /// Sends [type] into [broadcastId].
  ///
  /// A tap, not a write with an answer: the API broadcasts the stored reaction
  /// back to the room as `newReaction` rather than answering this frame, so
  /// there is nothing to await.
  void sendReaction({required Id broadcastId, required ReactionType type});
}
