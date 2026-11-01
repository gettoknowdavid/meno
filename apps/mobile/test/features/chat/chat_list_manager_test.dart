import 'dart:async';

import 'package:dio/dio.dart' show CancelToken;
import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/chat/chat.dart';

import '../../_support/pump_until_found.dart';
import '../../_support/stub_api.dart';

const _broadcastId = '3f2b1c9e-8a4d-4f6b-9c1e-2d3a4b5c6d7e';
const _foreignBroadcastId = '3f2b1c9e-8a4d-4f6b-9c1e-2d3a4b5c6d7f';
const _me = 'c0ffee00-1111-4222-8333-444455556666';
const _other = 'c0ffee00-2222-4222-8333-444455556666';

String _id(int n) => '018f2b1e-0000-7000-8000-${n.toString().padLeft(12, '0')}';

Message _message({
  required int id,
  String content = 'hello',
  String broadcastId = _broadcastId,
  Id? senderId,
}) => Message(
  id: Id.fromString(_id(id)),
  content: MultiLineString(content),
  broadcastId: Id.fromString(broadcastId),
  sender: MessageSender(
    id: senderId ?? Id.fromString(_other),
    fullName: SingleLineString('Ada'),
  ),
  createdAt: DateTime.now(),
);

CursorPage<Message> _page(List<Message> items, {String? nextCursor}) =>
    CursorPage(
      items: items,
      nextCursor: nextCursor,
      hasNextPage: nextCursor != null,
    );

/// The API's one chat read, answering from a script.
final class _FakeReader implements ChatMessagePageReader {
  new(this.pages);

  final List<CursorPage<Message>> pages;

  int reads = 0;
  final cursors = <String?>[];

  @override
  Future<CursorPage<Message>> messages(
    Id broadcastId, {
    String? cursor,
    int limit = ChatHttpService.defaultPageSize,
    CancelToken? cancelToken,
  }) async {
    reads += 1;
    cursors.add(cursor);
    final index = cursor == null ? 0 : int.tryParse(cursor) ?? 0;
    return pages[index.clamp(0, pages.length - 1)];
  }
}

/// The room's socket, driven by the test instead of a transport.
final class _FakeChannel implements ChatSocketChannel {
  final newMessage = StreamController<Message>.broadcast();
  final editedMessage = StreamController<Message>.broadcast();
  final deletedMessage = StreamController<DeletedMessageSignal>.broadcast();
  final reconnected = StreamController<void>.broadcast();

  @override
  Stream<Message> get onNewMessage => newMessage.stream;

  @override
  Stream<Message> get onEditedMessage => editedMessage.stream;

  @override
  Stream<DeletedMessageSignal> get onDeletedMessage => deletedMessage.stream;

  @override
  Stream<void> get onReconnected => reconnected.stream;

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

  Future<void> close() async {
    await newMessage.close();
    await editedMessage.close();
    await deletedMessage.close();
    await reconnected.close();
  }
}

void main() {
  late _FakeReader reader;
  late _FakeChannel channel;
  late ChatListManager manager;

  ChatListManager build(List<CursorPage<Message>> pages) => ChatListManager(
    http: reader = _FakeReader(pages),
    socket: channel,
    broadcastId: Id.fromString(_broadcastId),
    userId: Id.fromString(_me),
    logger: MockMenoLogger(),
  );

  /// Runs the room's start-up and waits for the first page to land.
  Future<void> start() async {
    manager.initialize.run();
    await pumpUntil(
      () => manager.feed.updateWasCalled && !manager.feed.isFetching.value,
    );
  }

  setUp(() {
    Command.globalExceptionHandler = (_, _) {};
    channel = _FakeChannel();
  });

  tearDown(() async {
    await manager.onDispose();
    await channel.close();
  });

  test('initialize subscribes first, then reads the newest page', () async {
    manager = build([
      _page([_message(id: 2), _message(id: 1)]),
    ]);

    await start();

    expect(reader.cursors, [null], reason: 'the first page is uncursored');
    expect(
      manager.feed.items.map((message) => message.id.getOrCrash),
      [_id(2), _id(1)],
      reason: 'the API answers newest first and the list draws it reversed',
    );
    expect(manager.updateWasCalled, isTrue);
  });

  test('initialize twice subscribes once', () async {
    manager = build([
      _page([_message(id: 1)]),
    ]);

    manager.initialize.run();
    manager.initialize.run();
    await pumpUntil(() => reader.reads == 2);
    await pumpUntil(() => !manager.feed.isFetching.value);

    channel.newMessage.add(_message(id: 2));
    await pumpUntil(() => manager.feed.items.length >= 2);
    await Future<void>.delayed(const Duration(milliseconds: 20));

    expect(manager.feed.items, hasLength(2));
  });

  test('a message from somebody else is unread news while hidden', () async {
    manager = build([
      _page([_message(id: 1)]),
    ]);
    await start();

    channel.newMessage.add(_message(id: 2));
    await pumpUntil(() => manager.feed.items.length == 2);

    expect(manager.feed.items.first.id.getOrCrash, _id(2));
    expect(manager.hasUnread.value, isTrue);
  });

  test('a message does not mark itself unread', () async {
    manager = build([
      _page([_message(id: 1)]),
    ]);
    await start();

    channel.newMessage.add(_message(id: 2, senderId: Id.fromString(_me)));
    await pumpUntil(() => manager.feed.items.length == 2);

    expect(manager.hasUnread.value, isFalse);
  });

  test('a message read on screen is not unread news', () async {
    manager = build([
      _page([_message(id: 1)]),
    ]);
    await start();
    manager.setVisible(true);

    channel.newMessage.add(_message(id: 2));
    await pumpUntil(() => manager.feed.items.length == 2);

    expect(manager.hasUnread.value, isFalse);
  });

  test('coming back to the chat clears the dot', () async {
    manager = build([
      _page([_message(id: 1)]),
    ]);
    await start();

    channel.newMessage.add(_message(id: 2));
    await pumpUntil(() => manager.hasUnread.value);
    manager.setVisible(true);

    expect(manager.hasUnread.value, isFalse);
    expect(manager.feed.items, hasLength(2), reason: 'the row is kept');
  });

  test('an edited message updates its row without adding one', () async {
    manager = build([
      _page([_message(id: 1, content: 'original'), _message(id: 2)]),
    ]);
    await start();
    final count = manager.feed.itemCount.value;
    final revision = manager.feed.revision.value;

    channel.editedMessage.add(_message(id: 1, content: 'fixed'));
    await pumpUntil(
      () => manager.feed.items.any(
        (message) => message.content.getOrNull == 'fixed',
      ),
    );

    expect(manager.feed.itemCount.value, count);
    expect(manager.feed.revision.value, revision + 1);
  });

  test('a deleted message leaves the list', () async {
    manager = build([
      _page([_message(id: 1), _message(id: 2)]),
    ]);
    await start();

    channel.deletedMessage.add(
      DeletedMessageSignal(
        messageId: _id(1),
        broadcastId: Id.fromString(_broadcastId),
      ),
    );
    await pumpUntil(() => manager.feed.items.length == 1);

    expect(manager.feed.items.single.id.getOrCrash, _id(2));
  });

  test('a frame from another room changes nothing', () async {
    manager = build([
      _page([_message(id: 1)]),
    ]);
    await start();

    channel.deletedMessage.add(
      DeletedMessageSignal(
        messageId: _id(1),
        broadcastId: Id.fromString(_foreignBroadcastId),
      ),
    );
    channel.newMessage.add(_message(id: 2, broadcastId: _foreignBroadcastId));
    await Future<void>.delayed(const Duration(milliseconds: 20));

    expect(manager.feed.items, hasLength(1));
    expect(manager.hasUnread.value, isFalse);
  });

  test('a reconnect reads the history again', () async {
    manager = build([
      _page([_message(id: 1)]),
    ]);
    await start();
    expect(reader.reads, 1);

    channel.reconnected.add(null);
    await pumpUntil(() => reader.reads == 2);

    expect(reader.cursors, [null, null], reason: 'a refresh starts over');
    expect(manager.feed.items, hasLength(1));
  });
}
