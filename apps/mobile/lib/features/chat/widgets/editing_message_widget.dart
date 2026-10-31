import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/broadcast/manager/active_broadcast_manager.dart';
import 'package:meno_ui/meno_ui.dart';

/// The banner that says which message is being edited, with a way out of it.
class EditingMessageWidget extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    final chat = di<ActiveBroadcastManager>().session?.chat;
    if (chat == null) return const SizedBox.shrink();

    final messageToEdit = watch(chat.messageToEdit).value;
    if (messageToEdit == null) return const SizedBox.shrink();

    return Container(
      constraints: const BoxConstraints(maxHeight: 80),
      width: double.infinity,
      padding: const .fromLTRB(16, 8, 16, 8),
      color: colors.disabledContainer,
      child: Row(
        children: [
          Expanded(
            child: Card(
              shape: Border(
                left: BorderSide(color: colors.secondary, width: 8),
              ),
              child: Padding(
                padding: const .fromLTRB(24, 4, 24, 4),
                child: Column(
                  mainAxisAlignment: .center,
                  crossAxisAlignment: .stretch,
                  children: [
                    MText(
                      'You',
                      style: textTheme.captionMedium,
                      color: colors.informational,
                    ),
                    Spaces.verticalMicro,
                    MText(
                      messageToEdit.content.getOrCrash,
                      maxLines: 1,
                      overflow: .ellipsis,
                      style: textTheme.bodyMedium,
                    ),
                  ],
                ),
              ),
            ),
          ),
          Spaces.horizontalSmall,
          IconButton(
            onPressed: chat.stopEditing.run,
            icon: const Icon(MIcons.x_close),
          ),
        ],
      ),
    );
  }
}
