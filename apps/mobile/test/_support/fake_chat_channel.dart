import 'dart:async';

import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/chat/chat.dart';

/// The room's socket, driven by a test instead of a transport.
///
/// Each event is a controller the test pushes through, and every frame the
/// client sends is recorded. Nothing here talks to a server: what the app does
/// with a reaction is the thing under test, not the wire.
final class FakeChatChannel implements ChatSocketChannel {
  final newMessage = StreamController<Message>.broadcast();
  final editedMessage = StreamController<Message>.broadcast();
  final deletedMessage = StreamController<DeletedMessageSignal>.broadcast();
  final reconnected = StreamController<void>.broadcast();
  final reaction = StreamController<ChatReaction>.broadcast();

  /// Every `sendReaction` this channel was asked to send, in order.
  final reactionsSent = <({Id broadcastId, ReactionType type})>[];

  @override
  Stream<Message> get onNewMessage => newMessage.stream;

  @override
  Stream<Message> get onEditedMessage => editedMessage.stream;

  @override
  Stream<DeletedMessageSignal> get onDeletedMessage => deletedMessage.stream;

  @override
  Stream<void> get onReconnected => reconnected.stream;

  @override
  Stream<ChatReaction> get onReaction => reaction.stream;

  @override
  void sendMessage({required Id broadcastId, required String content}) {}

  @override
  void editMessage({
    required Id broadcastId,
    required Id messageId,
    required String content,
  }) {}

  @override
  void deleteMessage({required Id broadcastId, required Id messageId}) {}

  @override
  void sendReaction({required Id broadcastId, required ReactionType type}) {
    reactionsSent.add((broadcastId: broadcastId, type: type));
  }

  Future<void> close() async {
    await newMessage.close();
    await editedMessage.close();
    await deletedMessage.close();
    await reconnected.close();
    await reaction.close();
  }
}
