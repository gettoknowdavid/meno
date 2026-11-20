import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/chat/chat.dart';

import '../../_support/fake_chat_channel.dart';
import '../../_support/stub_api.dart';

const _broadcastId = '3f2b1c9e-8a4d-4f6b-9c1e-2d3a4b5c6d7e';
const _me = 'c0ffee00-1111-4222-8333-444455556666';

void main() {
  testWidgets('a reaction floats over the stage and then leaves', (
    tester,
  ) async {
    final channel = FakeChatChannel();
    final manager = ReactionsManager(
      socket: channel,
      broadcastId: Id.fromString(_broadcastId),
      userId: Id.fromString(_me),
      logger: MockMenoLogger(),
    );

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Center(
            child: SizedBox(
              width: 320,
              height: 320,
              child: ReactionOverlay(reactions: manager),
            ),
          ),
        ),
      ),
    );

    expect(find.byType(FloatingReaction), findsNothing);

    manager.send(ReactionType.flame);
    await tester.pump();

    // Drawn in the frame the tap happened in, not when the server echoes.
    expect(find.byType(FloatingReaction), findsOneWidget);

    // It ignores pointers, so a tap that lands on it still reaches the stage.
    expect(
      find.descendant(
        of: find.byType(ReactionOverlay),
        matching: find.byType(IgnorePointer),
      ),
      findsWidgets,
    );

    await tester.pump(const Duration(seconds: 4));
    await tester.pump();

    expect(find.byType(FloatingReaction), findsNothing);

    // Unmounting proves the ticker and the subscription go down with the
    // widget. The manager's own disposal is covered by the manager test: a
    // widget test runs in a fake-async zone, where awaiting `onDispose()` would
    // wait on a microtask nothing drains, so it is not awaited here.
    await tester.pumpWidget(const SizedBox.shrink());
  });
}
