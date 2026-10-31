import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/chat/widgets/chat_input_container.dart';
import 'package:meno_mobile/features/chat/widgets/chat_list.dart';
import 'package:meno_mobile/features/chat/widgets/chat_welcome_widget.dart';
import 'package:meno_mobile/features/chat/widgets/editing_message_widget.dart';

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

    return LayoutBuilder(
      builder: (context, constraints) => Column(
        children: [
          Expanded(
            child: Align(
              alignment: Alignment.topCenter,
              child: ChatList(scrollController: scrollController),
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
