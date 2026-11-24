import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/broadcast/manager/_manager.dart';
import 'package:meno_mobile/features/chat/chat.dart';

/// The room's chat tab: history above, composer below.
///
/// The history is drawn top-aligned inside the expanded space so a short
/// conversation sits at the bottom of the screen instead of floating in the
/// middle of it, and the composer is inside the safe area because it is the one
/// part a keyboard covers.
class LiveChatTab extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final scrollController = createOnce(ScrollController.new);
    final manager = di<ActiveBroadcastManager>();
    final reactions = manager.session?.reactions;

    return LayoutBuilder(
      builder: (context, constraints) => Column(
        children: [
          Expanded(
            child: Align(
              alignment: Alignment.topCenter,
              child: Stack(
                children: [
                  ChatList(scrollController: scrollController),
                  if (reactions != null)
                    Positioned.fill(
                      child: ReactionOverlay(
                        key: const Key('StageReactionOverlay'),
                        reactions: reactions,
                      ),
                    ),
                ],
              ),
            ),
          ),
          SafeArea(
            child: Column(
              children: [
                const ChatWelcomeWidget(),
                const EditingMessageWidget(),
                ChatInputContainer(scrollController: scrollController),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
