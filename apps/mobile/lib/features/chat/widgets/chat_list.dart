import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/manager/active_broadcast_manager.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';
import 'package:meno_mobile/features/chat/widgets/chat_bubble.dart';
import 'package:meno_ui/meno_ui.dart';

/// The room's messages, newest at the bottom.
class ChatList extends WatchingWidget {
  const new({required this.scrollController, super.key});

  final ScrollController scrollController;

  @override
  Widget build(BuildContext context) {
    // Watched so a session that appears after this build is picked up: the
    // managers belong to the open room, not to the container.
    watchValue<ActiveBroadcastManager, LiveSnapshot>(
      (manager) => manager.snapshot,
    );

    final session = di<ActiveBroadcastManager>().session;
    final chatList = session?.chatList;
    final chat = session?.chat;
    if (chatList == null || chat == null) return const SizedBox.shrink();

    final feed = chatList.feed;
    final itemCount = watch(feed.itemCount).value;
    final isFetching = watch(feed.isFetching).value;
    // Watched, not read: an edit does not change the count.
    watch(feed.revision);

    // Selected from the feed rather than read off a container: the feed is the
    // room's, and the handler is typed by what the selector returns.
    registerHandler(
      target: feed,
      select: (source) => source.commandErrors,
      handler: (context, error, _) {
        if (error?.error == null) return;
        context.showErrorToast(errorMessage(error?.error));
      },
    );

    registerHandler(
      target: chat.sendMessage,
      allowObservableChange: true,
      handler: (_, _, _) => _scrollToBottom(),
    );

    registerHandler(
      target: chat.editMessage,
      allowObservableChange: true,
      handler: (_, _, _) => _scrollToBottom(),
    );

    if (!feed.updateWasCalled && isFetching) return const _LoadingIndicator();

    if (feed.updateWasCalled && itemCount == 0) return const SizedBox.shrink();

    final hostId = session!.broadcast.hostId;
    final userId = session.userId;

    return ListView.separated(
      controller: scrollController,
      padding: const .symmetric(vertical: Insets.lg),
      reverse: true,
      separatorBuilder: (context, _) => Spaces.verticalLarge,
      itemCount: itemCount + 1,
      itemBuilder: (context, index) {
        if (index == itemCount) {
          if (isFetching) return const _LoadingIndicator();
          if (feed.hasReachedEnd) return const _BeginningOfChatIndicator();
          return const SizedBox.shrink();
        }

        final message = feed.getItemAtIndex(index);
        return ChatBubble(
          message: message,
          isHost: message.senderId == hostId,
          isSentByMe: message.senderId == userId,
        );
      },
    );
  }

  /// Keeps the newest message in view after the person's own action.
  void _scrollToBottom() {
    if (!scrollController.hasClients) return;
    scrollController.animateTo(
      0,
      duration: const Duration(milliseconds: 300),
      curve: Curves.easeOut,
    );
  }
}

class _BeginningOfChatIndicator extends StatelessWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);
    return Padding(
      padding: const .all(16),
      child: Center(
        child: MText(
          'Beginning of chat',
          style: textTheme.captionRegular,
          color: colors.inActive.withValues(alpha: 0.6),
        ),
      ),
    );
  }
}

class _LoadingIndicator extends StatelessWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    return const Padding(
      padding: .all(16),
      child: Center(child: MLoadingIndicator.box()),
    );
  }
}
