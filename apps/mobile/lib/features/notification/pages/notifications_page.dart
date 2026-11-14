import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart' hide Notification;
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_mobile/features/notification/notification.dart';
import 'package:meno_mobile/features/notification/widgets/_widgets.dart';
import 'package:meno_ui/meno_ui.dart';

class NotificationsPage extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final manager = di<NotificationsManager>();
    return MScaffold(
      appBar: const _AppBar(),
      body: CustomScrollView(
        slivers: [
          // The invitations come first: they are the only thing in this page
          // that is waiting for the reader, and they are gone once answered.
          const SliverToBoxAdapter(
            child: Padding(
              padding: EdgeInsets.fromLTRB(Insets.lg, Insets.md, Insets.lg, 0),
              child: CohostInvitationList(),
            ),
          ),
          _NotificationsPageView(feedSource: manager.feed),
        ],
      ),
    );
  }
}

class _NotificationsPageView extends WatchingWidget {
  const new({required this.feedSource});

  final NotificationsFeedSource feedSource;

  @override
  Widget build(BuildContext context) {
    final grouped = watch(feedSource.groupedItems).value;
    final isFetching = watch(feedSource.isFetching).value;
    final isInitializing = !feedSource.updateWasCalled && isFetching;

    if (isInitializing) {
      return const SliverFillRemaining(
        hasScrollBody: false,
        child: Center(child: MLoadingIndicator.box()),
      );
    }

    if (grouped.isEmpty && feedSource.updateWasCalled) {
      return const SliverFillRemaining(
        hasScrollBody: false,
        child: MEmptyState(),
      );
    }

    return SliverMainAxisGroup(
      slivers: [
        SliverList.separated(
          separatorBuilder: (context, index) => Spaces.verticalLarge,
          itemCount: grouped.length,
          itemBuilder: (context, index) {
            // Auto-pagination: trigger near end based on underlying flat count
            feedSource.getItemAtIndex(
              (index / grouped.length * feedSource.items.length)
                  .clamp(0, feedSource.items.length - 1)
                  .toInt(),
            );

            return switch (grouped[index]) {
              NotificationSectionHeader(:final label) => _SectionHeader(
                label: label,
              ),
              NotificationListEntry(:final proxy) => NotificationCard(
                proxy: proxy,
              ),
            };
          },
        ),
        if (!isInitializing) ...[
          SliverToBoxAdapter(
            child: MPagedLoadingIndicator(
              isLoading: isFetching,
              hasMore: feedSource.hasNextPage,
            ),
          ),
        ],
      ],
    );
  }
}

class _SectionHeader extends StatelessWidget {
  const new({required this.label});

  final String label;

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);
    return Padding(
      padding: const .only(top: 16),
      child: MText(label, style: textTheme.subheadingBold),
    );
  }
}

class _AppBar extends StatelessWidget implements PreferredSizeWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    return AppBar(
      automaticallyImplyLeading: false,
      flexibleSpace: const Column(
        mainAxisAlignment: MainAxisAlignment.end,
        children: [
          Padding(
            padding: EdgeInsets.symmetric(horizontal: 12, vertical: 8),
            child: MBackButton.withText(),
          ),
          Padding(
            padding: EdgeInsets.symmetric(vertical: 8),
            child: MHeader(title: MText('Notifications')),
          ),
        ],
      ),
    );
  }

  @override
  Size get preferredSize => const Size.fromHeight(86);
}
