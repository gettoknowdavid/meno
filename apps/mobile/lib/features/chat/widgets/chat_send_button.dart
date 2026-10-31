import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/chat/manager/chat_manager.dart';
import 'package:meno_ui/meno_ui.dart';

/// Sends what has been typed, or saves the edit being composed.
///
/// It is drawn only when there is something sendable: an empty field has no
/// action, and a disabled circle is a control that says "almost" for no reason.
class ChatSendButton extends WatchingWidget {
  const new({
    required this.manager,
    required this.scrollController,
    super.key,
  });

  final ChatManager manager;
  final ScrollController scrollController;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);

    final content = watch(manager.content).value;

    if (!content.isValid || content.getOrElse((_) => '').isEmpty) {
      return const SizedBox.shrink();
    }

    final isEditing = watch(manager.isEditing).value;

    return Column(
      children: [
        Spaces.horizontalSmall,
        MIconButton(
          icon: const Icon(MIcons.send),
          isFilled: true,
          fillColor: colors.primary,
          color: colors.onPrimary,
          size: 40,
          iconSize: 20,
          onPressed: () {
            FocusScope.of(context).unfocus();
            isEditing ? manager.editMessage.run() : manager.sendMessage.run();
            scrollController.animateTo(
              0,
              duration: const Duration(milliseconds: 300),
              curve: Curves.easeInOut,
            );
          },
        ),
      ],
    );
  }
}
