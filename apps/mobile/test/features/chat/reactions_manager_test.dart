import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/model/socket.dart';
import 'package:meno_mobile/features/chat/chat.dart';

import '../../_support/fake_chat_channel.dart';
import '../../_support/stub_api.dart';

const _broadcastId = '3f2b1c9e-8a4d-4f6b-9c1e-2d3a4b5c6d7e';
const _me = 'c0ffee00-1111-4222-8333-444455556666';
const _other = 'c0ffee00-2222-4222-8333-444455556666';
const _reactionId = '018f2b1e-0000-7000-8000-000000000001';

ChatReaction _reaction({
  String content = 'clap',
  String broadcastId = _broadcastId,
  String senderId = _other,
}) => ChatReaction(
  id: _reactionId,
  broadcastId: Id.fromString(broadcastId),
  senderId: Id.fromString(senderId),
  content: content,
);

void main() {
  late FakeChatChannel channel;
  late ReactionsManager manager;
  late List<ReactionBurst> bursts;

  setUp(() {
    channel = FakeChatChannel();
    manager = ReactionsManager(
      socket: channel,
      broadcastId: Id.fromString(_broadcastId),
      userId: Id.fromString(_me),
      logger: MockMenoLogger(),
      sendInterval: const Duration(milliseconds: 80),
    );
    bursts = [];
    manager.bursts.listen(bursts.add);
  });

  tearDown(() async {
    await manager.onDispose();
    await channel.close();
  });

  test('a tap is drawn at once and sent once', () {
    manager.send(ReactionType.clap);

    expect(bursts, hasLength(1));
    expect(bursts.single.type, ReactionType.clap);
    expect(bursts.single.mine, isTrue);

    expect(channel.reactionsSent, hasLength(1));
    expect(channel.reactionsSent.single.type, ReactionType.clap);
    expect(channel.reactionsSent.single.broadcastId.getOrNull, _broadcastId);
  });

  test('a burst of taps is drawn in full and sent as a rate', () async {
    manager
      ..send(ReactionType.clap)
      ..send(ReactionType.flame)
      ..send(ReactionType.like);

    // Every tap is animated: the strip is local and costs nothing.
    expect(bursts.map((burst) => burst.type), [
      ReactionType.clap,
      ReactionType.flame,
      ReactionType.like,
    ]);

    // One frame leaves at once; the rest wait for the window to close.
    expect(channel.reactionsSent, hasLength(1));
    expect(channel.reactionsSent.single.type, ReactionType.clap);

    await Future<void>.delayed(const Duration(milliseconds: 200));

    // The trailing frame carries the last tap, so the burst still arrives.
    expect(channel.reactionsSent, hasLength(2));
    expect(channel.reactionsSent.last.type, ReactionType.like);
  });

  test("another account's reaction is drawn", () async {
    channel.reaction.add(_reaction(content: 'flame'));
    await Future<void>.delayed(Duration.zero);

    expect(bursts, hasLength(1));
    expect(bursts.single.type, ReactionType.flame);
    expect(bursts.single.mine, isFalse);
    expect(bursts.single.id, _reactionId);
  });

  test('the sender does not draw its own frame a second time', () async {
    channel.reaction.add(_reaction(senderId: _me));
    await Future<void>.delayed(Duration.zero);

    // The tap already drew it; the API echoes to the sender too.
    expect(bursts, isEmpty);
  });

  test('a label this build cannot draw and another room are dropped', () async {
    channel.reaction
      ..add(_reaction(content: 'dance'))
      ..add(_reaction(broadcastId: '018f2b1e-0000-7000-8000-000000000009'));
    await Future<void>.delayed(Duration.zero);

    expect(bursts, isEmpty);
  });

  test('an emoji from another client draws the same five', () async {
    channel.reaction.add(_reaction(content: '🔥'));
    await Future<void>.delayed(Duration.zero);

    expect(bursts.single.type, ReactionType.flame);
  });

  test('the frame the API emits is the socket event this build knows', () {
    final frame = SocketFrame.tryDecode(
      jsonEncode({
        'event': 'newReaction',
        'data': {
          'id': _reactionId,
          'broadcastId': _broadcastId,
          'senderId': _other,
          'content': 'clap',
          'createdAt': '2026-11-01T10:00:00Z',
        },
      }),
    );

    expect(frame, isNotNull);
    expect(frame!.event, SocketEvent.newReaction);

    final reaction = parseChatReaction(
      frame.data! as Map<String, dynamic>,
    );
    expect(reaction.type, ReactionType.clap);
    expect(reaction.broadcastId.getOrNull, _broadcastId);
  });

  test('a tap after disposal is dropped instead of thrown', () async {
    await manager.onDispose();

    manager.send(ReactionType.snap);

    expect(bursts, isEmpty);
    expect(channel.reactionsSent, isEmpty);
  });
}
