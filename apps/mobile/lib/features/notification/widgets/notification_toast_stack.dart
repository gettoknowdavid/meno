import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart' hide Notification;
import 'package:meno_mobile/features/notification/notification.dart';
import 'package:meno_ui/meno_ui.dart';

class NotificationToastStack extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final notifications = watchValue<NotificationsManager, List<Notification>>(
      (manager) => manager.recentNotifications,
    );

    if (notifications.isEmpty) return const SizedBox.shrink();

    // Stack height grows slightly per layer so back cards peek through
    final stackHeight = 64.0 + ((notifications.length - 1) * 10.0);

    return GestureDetector(
      onTap: di<NotificationsManager>().dismissToasts,
      child: SizedBox(
        height: stackHeight,
        child: Stack(
          clipBehavior: Clip.none,
          children: [
            // Render back-to-front so index 0 (latest) paints on top
            for (int i = notifications.length - 1; i >= 0; i--)
              _AnimatedToastCard(
                key: ValueKey(notifications[i].id ?? i.toString()),
                notification: notifications[i],
                index: i,
              ),
          ],
        ),
      ),
    );
  }
}

class _AnimatedToastCard extends StatefulWidget {
  const new({required this.notification, required this.index, super.key});

  final Notification notification;
  final int index;

  @override
  State<_AnimatedToastCard> createState() => _AnimatedToastCardState();
}

class _AnimatedToastCardState extends State<_AnimatedToastCard>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller;
  late final Animation<double> _scale;
  late final Animation<double> _opacity;

  @override
  void initState() {
    super.initState();
    _controller = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 320),
    );
    _scale = CurvedAnimation(parent: _controller, curve: Curves.easeOutBack);
    _opacity = CurvedAnimation(parent: _controller, curve: Curves.easeOut);
    _controller.forward();
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final i = widget.index;

    // Each layer behind is narrower and shifted down
    final scaleX = 1.0 - (i * 0.06);
    final offsetY = i * 10.0;

    return Positioned(
      top: 0,
      left: 0,
      right: 0,
      child: Transform.translate(
        offset: Offset(0, offsetY),
        child: Transform.scale(
          scaleX: scaleX,
          scaleY: 1,
          child: FadeTransition(
            opacity: i == 0
                ? _opacity
                : AlwaysStoppedAnimation(1.0 - (i * 0.2)),
            child: ScaleTransition(
              scale: i == 0 ? _scale : const AlwaysStoppedAnimation(1),
              alignment: Alignment.topCenter,
              child: _ToastCard(notification: widget.notification),
            ),
          ),
        ),
      ),
    );
  }
}

class _ToastCard extends StatelessWidget {
  const new({required this.notification});

  final Notification notification;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);
    final content = notification.content;

    final (imageUrl, title, subtitle) = switch (notification.type) {
      NotificationType.userSubscribed => (
        content?.subscriberImageUrl,
        content?.subscriberName ?? '',
        'subscribed to you',
      ),
      NotificationType.liveBroadcastStarted => (
        content?.broadcastImageUrl,
        content?.broadcastCreator ?? '',
        'is now live: "${content?.broadcastTitle ?? ''}"',
      ),
      NotificationType.addedAsCoHost => (
        content?.cohostImageUrl,
        content?.cohostFullName ?? '',
        'added you as co-host',
      ),
      _ => (null, '', ''),
    };

    return Card(
      margin: .zero,
      shape: const RoundedSuperellipseBorder(
        borderRadius: BorderRadiusGeometry.all(.circular(16)),
      ),
      child: Padding(
        padding: const .symmetric(horizontal: Insets.lg, vertical: Insets.md),
        child: Row(
          children: [
            MAvatar(radius: 18, url: imageUrl),
            Spaces.horizontalSmall,
            Expanded(
              child: Column(
                crossAxisAlignment: .start,
                mainAxisSize: .min,
                children: [
                  MText(
                    title,
                    style: textTheme.captionMedium,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                  MText(
                    subtitle,
                    style: textTheme.microRegular,
                    color: colors.onBackground.withValues(alpha: 0.6),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}
