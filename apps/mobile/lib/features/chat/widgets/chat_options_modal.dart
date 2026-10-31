import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/broadcast/manager/active_broadcast_manager.dart';
import 'package:meno_mobile/features/chat/manager/chat_manager.dart';
import 'package:meno_mobile/features/chat/model/_model.dart';
import 'package:meno_mobile/features/chat/widgets/delete_comment_alert_dialog.dart';
import 'package:meno_ui/meno_ui.dart';

/// What may be done to one message.
class ChatMessageOptionsModal extends StatelessWidget {
  const new _({
    required this.message,
    this.isHost = false,
    this.isSentByMe = false,
    super.key,
  });

  /// The message this sheet is about.
  final Message message;

  /// Whether the message was sent by the person who created the broadcast.
  final bool isHost;

  /// Whether the reader wrote it.
  final bool isSentByMe;

  static Future<dynamic> show(
    BuildContext context, {
    required Message message,
    bool isHost = false,
    bool isSentByMe = false,
  }) {
    return showModalBottomSheet<dynamic>(
      context: context,
      isScrollControlled: true,
      useRootNavigator: true,
      builder: (ctx) => ChatMessageOptionsModal._(
        key: ObjectKey(message),
        message: message,
        isHost: isHost,
        isSentByMe: isSentByMe,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final chat = di<ActiveBroadcastManager>().session?.chat;

    return MModal(
      title: isSentByMe ? 'My Comment' : "User's Comment",
      builder: (context) => Column(
        crossAxisAlignment: .stretch,
        mainAxisSize: .min,
        spacing: Insets.lg,
        children: [
          if (chat != null && message.isEditable)
            MModalListTile(
              leading: const Icon(MIcons.edit_05),
              title: 'Edit',
              onTap: () {
                chat.startEditing.run(message);
                context.pop<void>();
              },
            ),
          if (chat != null && (isHost || isSentByMe) && message.isDeletable)
            MModalListTile(
              leading: Icon(MIcons.trash, color: colors.error),
              title: 'Delete',
              onTap: () => handleDeleteMessage(context, chat),
              titleColor: colors.error,
            ),
          if (!isSentByMe)
            MModalListTile(
              leading: const Icon(Icons.flag),
              title: 'Report',
              onTap: () {},
            ),
        ],
      ),
    );
  }

  Future<void> handleDeleteMessage(
    BuildContext context,
    ChatManager chat,
  ) async {
    final result = await DeleteCommentAlertDialog.show(context);
    if (result != true) return;
    if (!context.mounted) return;
    chat.deleteMessage.run(message);
  }
}
