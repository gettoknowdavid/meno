import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/chat/model/_model.dart';
import 'package:meno_mobile/features/chat/services/chat_socket_service.dart';

/// What the room's composer holds and what it does with it.
final class ChatManager implements Disposable {
  /// Creates the composer for one room, wired to its socket.
  new({
    required this._socket,
    required this._broadcastId,
    required this._userId,
    required this._logger,
  }) {
    sendMessage = .createAsyncNoParamNoResult(
      () async {
        final body = content.value.getOrCrash;
        _socket.sendMessage(broadcastId: _broadcastId, content: body);
        content.value = MultiLineString.empty;
      },
      errorFilterFn: menoExceptionFilter,
      restriction: content.map((value) => !value.isValid),
      debugName: 'chat.sendMessage',
    );

    startEditing = .createSyncNoResult<Message>(
      (message) {
        messageToEdit.value = message;
        content.value = message.content;
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'chat.startEditing',
    );

    stopEditing = .createSyncNoParamNoResult(
      () {
        messageToEdit.value = null;
        content.value = MultiLineString.empty;
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'chat.stopEditing',
    );

    editMessage = .createAsyncNoParamNoResult(
      () async {
        final message = messageToEdit.value;
        if (message == null || !message.isEditable) return;

        _socket.editMessage(
          broadcastId: _broadcastId,
          messageId: message.id,
          content: content.value.getOrCrash,
        );

        messageToEdit.value = null;
        content.value = MultiLineString.empty;
      },
      errorFilterFn: menoExceptionFilter,
      // An edit needs both halves: a message chosen and something valid typed.
      restriction: messageToEdit.combineLatest(
        content,
        (message, body) => message == null || !body.isValid,
      ),
      debugName: 'chat.editMessage',
    );

    deleteMessage = .createAsyncNoResult<Message>(
      (message) async {
        if (!message.isDeletable) return;
        _socket.deleteMessage(broadcastId: _broadcastId, messageId: message.id);
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'chat.deleteMessage',
    );
  }

  final ChatSocketChannel _socket;

  /// The room this composer writes into.
  final Id _broadcastId;

  /// The account whose messages are "mine".
  final Id _userId;

  final MenoLogger _logger;

  /// What has been typed but not sent.
  final ValueNotifier<MultiLineString> content = ValueNotifier(
    MultiLineString.empty,
  );

  /// The message being edited, or null when the composer is writing a new one.
  final messageToEdit = ValueNotifier<Message?>(null);

  /// Whether the "welcome to the live chat" note is still on screen.
  final ValueNotifier<bool> welcomeMessageVisible = ValueNotifier(true);

  /// Whether the composer is editing rather than writing.
  late final ValueListenable<bool> isEditing = messageToEdit.map(
    (message) => message != null,
  );

  /// The account writing in this room.
  Id get userId => _userId;

  /// Posts what has been typed and empties the field.
  late final Command<void, void> sendMessage;

  /// Takes `message` into the composer for editing.
  late final Command<Message, void> startEditing;

  /// Abandons an edit and empties the field.
  late final Command<void, void> stopEditing;

  /// Saves the edit being composed.
  late final Command<void, void> editMessage;

  /// Deletes `message`.
  late final Command<Message, void> deleteMessage;

  /// Records what was typed.
  ///
  /// Not a command: a keystroke is not an action with a result, a restriction
  /// or a failure, and routing every character through one would allocate a
  /// command run per key press.
  void onContentChange(String value) => content.value = MultiLineString(value);

  /// Dismisses the welcome note.
  void hideWelcomeNote() => welcomeMessageVisible.value = false;

  @override
  FutureOr<dynamic> onDispose() async {
    _logger.d('ChatManager: disposed', tag: MenoLogTag.chat.label);

    content.dispose();
    messageToEdit.dispose();
    welcomeMessageVisible.dispose();

    sendMessage.dispose();
    startEditing.dispose();
    stopEditing.dispose();
    editMessage.dispose();
    deleteMessage.dispose();
  }
}
