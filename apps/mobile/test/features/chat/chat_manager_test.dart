import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/chat/chat.dart';

import '../../_support/stub_api.dart';

const _broadcastId = '3f2b1c9e-8a4d-4f6b-9c1e-2d3a4b5c6d7e';
const _me = 'c0ffee00-1111-4222-8333-444455556666';
const _other = 'c0ffee00-2222-4222-8333-444455556666';

String _id(int n) => '018f2b1e-0000-7000-8000-${n.toString().padLeft(12, '0')}';

Message _message({
  int id = 1,
  String content = 'hello',
  String broadcastId = _broadcastId,
  Id? senderId,
  DateTime? createdAt,
}) => Message(
  id: Id.fromString(_id(id)),
  content: MultiLineString(content),
  broadcastId: Id.fromString(broadcastId),
  sender: MessageSender(
    id: senderId ?? Id.fromString(_other),
    fullName: SingleLineString('Ada'),
  ),
  createdAt: createdAt ?? DateTime.now(),
);

/// The room's socket, driven by the test instead of a transport.
final class _FakeChannel implements ChatSocketChannel {
  final _newMessage = StreamController<Message>.broadcast();
  final _editedMessage = StreamController<Message>.broadcast();
  final _deletedMessage = StreamController<DeletedMessageSignal>.broadcast();
  final _reconnected = StreamController<void>.broadcast();

  final sent = <({Id broadcastId, String content})>[];
  final edits = <({Id broadcastId, Id messageId, String content})>[];
  final deletes = <({Id broadcastId, Id messageId})>[];

  @override
  Stream<Message> get onNewMessage => _newMessage.stream;

  @override
  Stream<Message> get onEditedMessage => _editedMessage.stream;

  @override
  Stream<DeletedMessageSignal> get onDeletedMessage => _deletedMessage.stream;

  @override
  Stream<void> get onReconnected => _reconnected.stream;

  @override
  void sendMessage({required Id broadcastId, required String content}) {
    sent.add((broadcastId: broadcastId, content: content));
  }

  @override
  void editMessage({
    required Id broadcastId,
    required Id messageId,
    required String content,
  }) {
    edits.add((
      broadcastId: broadcastId,
      messageId: messageId,
      content: content,
    ));
  }

  @override
  void deleteMessage({required Id broadcastId, required Id messageId}) {
    deletes.add((broadcastId: broadcastId, messageId: messageId));
  }

  Future<void> close() async {
    await _newMessage.close();
    await _editedMessage.close();
    await _deletedMessage.close();
    await _reconnected.close();
  }
}

void main() {
  late _FakeChannel channel;
  late ChatManager manager;

  setUp(() {
    Command.globalExceptionHandler = (_, _) {};
    channel = _FakeChannel();
    manager = ChatManager(
      socket: channel,
      broadcastId: Id.fromString(_broadcastId),
      userId: Id.fromString(_me),
      logger: MockMenoLogger(),
    );
  });

  tearDown(() async {
    await manager.onDispose();
    await channel.close();
  });

  test('sendMessage posts what was typed and empties the composer', () async {
    manager.onContentChange('  hello there  ');

    await manager.sendMessage.runAsync();

    expect(channel.sent.single.broadcastId.getOrCrash, _broadcastId);
    expect(channel.sent.single.content, 'hello there');
    expect(manager.content.value.isValid, isFalse, reason: 'the field cleared');
  });

  test('sendMessage cannot run while the composer is empty', () {
    expect(manager.sendMessage.canRun.value, isFalse);

    manager.onContentChange('hello');

    expect(manager.sendMessage.canRun.value, isTrue);
  });

  test('startEditing fills the composer and stopEditing empties it', () {
    final message = _message(content: 'original');

    manager.startEditing.run(message);

    expect(manager.isEditing.value, isTrue);
    expect(manager.messageToEdit.value, message);
    expect(manager.content.value.getOrNull, 'original');

    manager.stopEditing.run();

    expect(manager.isEditing.value, isFalse);
    expect(manager.messageToEdit.value, isNull);
    expect(manager.content.value.isValid, isFalse);
  });

  test('editMessage sends the replacement and leaves the edit', () async {
    manager.startEditing.run(_message(content: 'original'));
    manager.onContentChange('fixed');

    await manager.editMessage.runAsync();

    final edit = channel.edits.single;
    expect(edit.broadcastId.getOrCrash, _broadcastId);
    expect(edit.messageId.getOrCrash, _id(1));
    expect(edit.content, 'fixed');
    expect(manager.messageToEdit.value, isNull);
    expect(manager.content.value.isValid, isFalse);
  });

  test('editMessage cannot run without a message chosen', () {
    manager.onContentChange('fixed');

    expect(manager.editMessage.canRun.value, isFalse);

    manager.startEditing.run(_message());

    expect(manager.editMessage.canRun.value, isTrue);
  });

  test('deleteMessage sends the id of a message still in its window', () async {
    final message = _message();

    await manager.deleteMessage.runAsync(message);

    expect(channel.deletes.single.broadcastId.getOrCrash, _broadcastId);
    expect(channel.deletes.single.messageId, message.id);
  });

  test('deleteMessage sends nothing for a message past its window', () async {
    final stale = _message(
      createdAt: DateTime.now().subtract(const Duration(minutes: 45)),
    );

    await manager.deleteMessage.runAsync(stale);

    expect(channel.deletes, isEmpty);
  });

  test('hideWelcomeNote dismisses the note once', () {
    expect(manager.welcomeMessageVisible.value, isTrue);

    manager.hideWelcomeNote();

    expect(manager.welcomeMessageVisible.value, isFalse);
    expect(manager.userId.getOrCrash, _me);
  });
}
