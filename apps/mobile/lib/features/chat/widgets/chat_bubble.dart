import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/chat/model/_model.dart';
import 'package:meno_mobile/features/chat/widgets/chat_options_modal.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:timeago/timeago.dart' as timeago;

/// One message, as the room draws it.
///
/// Presentational on purpose: who the host is and who the reader is come in as
/// flags from the list, which is the only place that has the room. The bubble
/// itself knows how to open the message's options and the sender's card, and
/// nothing else.
class ChatBubble extends StatelessWidget {
  const new({
    required this.message,
    this.isHost = false,
    this.isSentByMe = false,
    super.key,
  });

  final Message message;

  /// Whether the message's sender created the broadcast.
  final bool isHost;

  /// Whether the account reading this wrote it.
  final bool isSentByMe;

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      onLongPress: () => ChatMessageOptionsModal.show(
        context,
        message: message,
        isHost: isHost,
        isSentByMe: isSentByMe,
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 16),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.end,
          children: [
            _UserImage(
              key: const Key('chat-user-image'),
              imageUrl: message.imageUrl,
              onShowUserInfo: () => showUserInfo(context),
            ),
            Spaces.horizontalSmall,
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  _ChatMessageInfo(
                    key: const Key('chat-message-info'),
                    message: message,
                    onShowUserInfo: () => showUserInfo(context),
                    isHost: isHost,
                  ),
                  Spaces.verticalMicro,
                  _ChatContent(
                    key: const Key('chat-message-content'),
                    content: message.content.getOrElse((_) => ''),
                    isHost: isHost,
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  Future<dynamic> showUserInfo(BuildContext ctx) async {
    if (isSentByMe) return null;
    return await ctx.showModal(
      MUserInfoModal(
        fullName: message.senderName.getOrElse((_) => ''),
        imageUrl: message.imageUrl,
        onSubscribe: () {},
        onViewAccount: () async {
          await ctx.push(MRoutes.userProfileRoute(message.senderId));
        },
      ),
      isScrollControlled: true,
    );
  }
}

class _UserImage extends StatelessWidget {
  const new({required this.imageUrl, required this.onShowUserInfo, super.key});

  final String? imageUrl;
  final VoidCallback onShowUserInfo;

  @override
  Widget build(BuildContext context) {
    return LimitedBox(
      key: key,
      maxHeight: 24,
      maxWidth: 24,
      child: MAvatar(
        radius: 12,
        url: imageUrl,
        hasBorder: false,
        onTap: onShowUserInfo,
      ),
    );
  }
}

class _ChatMessageInfo extends StatelessWidget {
  const new({
    required this.message,
    required this.onShowUserInfo,
    required this.isHost,
    super.key,
  });

  final Message message;
  final VoidCallback onShowUserInfo;
  final bool isHost;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    final timeStamp = timeago.format(message.updatedAt ?? message.createdAt);

    return Row(
      key: key,
      mainAxisSize: MainAxisSize.min,
      children: [
        InkWell(
          onTap: onShowUserInfo,
          child: MText(
            message.senderName.getOrElse((_) => ''),
            style: textTheme.microMedium,
            color: colors.onBackgroundVariant,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
          ),
        ),
        Spaces.horizontalMicro,
        if (isHost) ...[
          MDot(dimension: 2, color: colors.onBackgroundVariant),
          Spaces.horizontalMicro,
          MText(
            'Host',
            style: textTheme.microMedium,
            color: colors.onBackgroundVariant,
          ),
          Spaces.horizontalMicro,
        ],
        MDot(dimension: 2, color: colors.onBackgroundVariant),
        Spaces.horizontalMicro,
        MText(
          message.isEdited ? 'Edited $timeStamp' : timeStamp,
          style: textTheme.microMedium,
          color: colors.onBackgroundVariant,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
        ),
      ],
    );
  }
}

class _ChatContent extends StatelessWidget {
  const new({required this.content, required this.isHost, super.key});

  final String content;
  final bool isHost;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return Container(
      key: key,
      padding: const EdgeInsets.all(Insets.md),
      decoration: ShapeDecoration(
        color: isHost ? colors.secondaryContainer : colors.surfaceShade,
        shape: const RoundedSuperellipseBorder(
          borderRadius: BorderRadiusGeometry.only(
            topLeft: Radius.circular(20),
            topRight: Radius.circular(20),
            bottomRight: Radius.circular(20),
          ),
        ),
      ),
      child: MText(
        content,
        style: textTheme.captionRegular,
        color: isHost ? colors.onSecondaryContainer : colors.onPrimaryContainer,
      ),
    );
  }
}
