import 'package:flutter/services.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/auth/manager/user_manager.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_mobile/features/profile/profile.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:share_plus/share_plus.dart';

/// One broadcast's own screen: what it is, who made it, and what it says.
///
/// The manager is built here rather than resolved from the locator because a
/// details screen is about one broadcast id, and the id is only known to the
/// route that opened it. [createOnce] gives it that screen's lifetime.
class BroadcastDetailsPage extends WatchingWidget {
  const new({required this.broadcastId, super.key});

  final Id broadcastId;

  @override
  Widget build(BuildContext context) {
    final manager = createOnce(
      () => BroadcastDetailsManager(
        http: di<BroadcastHttpService>(),
        userId: di<UserManager>().currentUserId.value ?? .empty,
        broadcastId: broadcastId,
        logger: di<MenoLogger>(),
      ),
    );

    callOnce((_) => manager.fetch.run());

    final isFetching = watch(manager.fetch.isRunning).value;
    final proxy = watch(manager.proxy).value;
    final error = watch(manager.fetch.errors).value;

    if (isFetching) {
      return MScaffold(
        appBar: MAppBar.secondary(title: ''),
        body: const Center(child: MLoadingIndicator.box()),
      );
    }

    if (!isFetching && error != null) {
      return MScaffold(
        appBar: MAppBar.secondary(title: 'An error occurred'),
        body: MErrorState(
          message: errorMessage(error.error),
          onRetry: manager.fetch.runAsync,
        ),
      );
    }

    if (proxy == null) {
      return MScaffold(
        appBar: MAppBar.secondary(title: 'Not Found'),
        body: MErrorState(
          message: 'No broadcast found. Please try again.',
          onRetry: manager.fetch.runAsync,
        ),
      );
    }
    //
    return BroadcastDetailsView(proxy: proxy);
  }
}

class BroadcastDetailsView extends WatchingWidget {
  const new({required this.proxy, super.key});

  final BroadcastProxy proxy;

  @override
  Widget build(BuildContext context) {
    watch(proxy);

    registerHandler(
      target: proxy,
      select: (manager) => manager.toggleIsFavourite.errors,
      handler: (context, error, _) {
        if (error?.error == null) return;
        context.showErrorToast(errorMessage(error?.error));
      },
    );

    return MScaffold(
      appBar: MAppBar.secondary(
        title: proxy.title.orEmpty,
        actions: [
          MIconButton(
            icon: proxy.isFavourited
                ? const Icon(Icons.star)
                : const Icon(Icons.star_border),
            onPressed: proxy.toggleIsFavourite.run,
          ),
          Spaces.horizontalLarge,
          MIconButton(
            icon: const Icon(MIcons.dots_horizontal),
            onPressed: () => _OptionsModal.show(
              context,
              proxy,
              onGoToProfile: () => _goToProfile(context, proxy),
              onUnsubscribe: () => _unsubscribe(context, proxy),
              onListenLater: () => _listenLater(context, proxy),
              onShare: () => _share(context, proxy),
              onCopyLink: () => _copyLink(context, proxy),
            ),
          ),
          Spaces.horizontalLarge,
        ],
      ),
      body: SingleChildScrollView(
        child: Column(
          children: [
            Spaces.verticalLarge,
            Align(child: _Artwork(imageUrl: proxy.image?.getUrl)),
            Spaces.verticalSmall,
            _Title(title: proxy.title.orEmpty),
            Spaces.verticalMicro,
            _Creator(
              name: proxy.creatorName.orEmpty,
              onPressed: () => _goToProfile(context, proxy),
            ),
            Spaces.verticalLarge,
            // Restreaming has no API: `POST /broadcasts/{id}/live` refuses a
            // broadcast that has ended, and there is no endpoint that copies a
            // finished broadcast into a new one. The control stays visible in
            // its designed place, disabled, rather than opening an invented
            // flow.
            Tooltip(
              message: 'Restream is not available yet.',
              child: MPrimaryButton.icon(
                label: 'Restream',
                icon: const Icon(MIcons.play),
                disabled: true,
                onPressed: () {},
              ),
            ),
            Spaces.verticalLarge,
            const MDivider(),
            Spaces.verticalXLarge,
            _Description(description: proxy.description.orEmpty),
          ],
        ),
      ),
    );
  }

  Future<void> _goToProfile(BuildContext context, BroadcastProxy proxy) async {
    if (!proxy.hostId.isValid) {
      context.showErrorToast('This broadcast has no creator to open.');
      return;
    }
    await context.push(MRoutes.userProfileRoute(proxy.hostId));
  }

  Future<void> _unsubscribe(BuildContext context, BroadcastProxy proxy) async {
    final hostId = proxy.hostId;
    if (!hostId.isValid) {
      context.showErrorToast(
        'This broadcast has no creator to unsubscribe from.',
      );
      return;
    }

    try {
      await di<ProfileHttpService>().unfollow(hostId);

      // The profile page and every card behind this one share the proxy, so
      // the button they draw has to agree with the call that just succeeded.
      if (di.isRegistered<UserProfileRepository>()) {
        final profiles = di<UserProfileRepository>();
        final held = profiles.acquire(hostId);
        try {
          final target = held.targetOrNull;
          if (target != null && held.isFollowing) {
            profiles.apply(held, target.copyWith(isFollowing: false));
          }
        } finally {
          profiles.release(held);
        }
      }

      if (context.mounted) context.showSuccessToast('Unsubscribed');
    } on Object catch (error) {
      if (context.mounted) context.showErrorToast(errorMessage(error));
    }
  }

  Future<void> _listenLater(BuildContext context, BroadcastProxy proxy) async {
    final wasFavourited = proxy.isFavourited;
    await proxy.toggleIsFavourite.runAsync();

    // The command rolls the flag back when the local write fails, and its
    // handler above has already said why.
    if (proxy.isFavourited == wasFavourited) return;

    if (di.isRegistered<FavouritesManager>()) {
      di<FavouritesManager>().fetch.run();
    }
    if (!context.mounted) return;

    context.showSuccessToast(
      proxy.isFavourited
          ? 'Added to Listen Later'
          : 'Removed from Listen Later',
    );
  }

  Future<void> _share(BuildContext pageContext, BroadcastProxy proxy) async {
    final title = proxy.title.orEmpty;
    final link = MRoutes.deepLink(MRoutes.broadcastRoute(proxy.id));
    final text = title.isEmpty ? link : '$title\n$link';

    try {
      await SharePlus.instance.share(
        ShareParams(
          text: text,
          subject: title.isEmpty ? null : title,
          sharePositionOrigin: _shareOrigin(pageContext),
        ),
      );
    } on Object catch (error) {
      if (pageContext.mounted) {
        pageContext.showErrorToast(errorMessage(error));
      }
    }
  }

  Future<void> _copyLink(BuildContext context, BroadcastProxy proxy) async {
    final link = MRoutes.deepLink(MRoutes.broadcastRoute(proxy.id));
    await Clipboard.setData(ClipboardData(text: link));
    if (context.mounted) context.showSuccessToast('Link copied');
  }
}

/// Where the iPad and macOS share popover should point.
Rect? _shareOrigin(BuildContext context) {
  final box = context.findRenderObject();
  if (box is! RenderBox || !box.hasSize) return null;
  return box.localToGlobal(Offset.zero) & box.size;
}

class _Artwork extends StatelessWidget {
  const new({this.imageUrl});

  final String? imageUrl;

  @override
  Widget build(BuildContext context) {
    return MBroadcastArtwork(imageUrl: imageUrl);
  }
}

class _Title extends StatelessWidget {
  const new({required this.title});

  final String title;

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);
    return SizedBox(
      height: 24,
      child: MText(
        title,
        style: textTheme.subheadingMedium,
        textAlign: .center,
        maxLines: 1,
        overflow: .ellipsis,
      ),
    );
  }
}

class _Creator extends StatelessWidget {
  const new({required this.name, this.onPressed});

  final String name;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: 16,
      child: MCreatorText(name: name, onPressed: onPressed),
    );
  }
}

class _Description extends StatelessWidget {
  const new({this.description});

  final String? description;

  @override
  Widget build(BuildContext context) {
    if (description == null) return const SizedBox.shrink();

    final textTheme = MTextTheme.of(context);
    return Column(
      children: [
        Row(
          children: [
            const Icon(MIcons.menu_03, size: 16),
            Spaces.horizontalSmall,
            MText('About Broadcast', style: textTheme.subheadingMedium),
          ],
        ),
        Spaces.verticalLarge,
        Align(alignment: .centerLeft, child: MText(description!)),
      ],
    );
  }
}

class _OptionsModal extends StatelessWidget {
  const new _({
    required this.proxy,
    required this.onGoToProfile,
    required this.onUnsubscribe,
    required this.onListenLater,
    required this.onShare,
    required this.onCopyLink,
  }) : super(key: null);

  final BroadcastProxy proxy;
  final VoidCallback onGoToProfile;
  final Future<void> Function() onUnsubscribe;
  final Future<void> Function() onListenLater;
  final Future<void> Function() onShare;
  final Future<void> Function() onCopyLink;

  @override
  Widget build(BuildContext context) {
    final broadcast = proxy.broadcast;
    return MModal(
      builder: (context) => Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          _Artwork(imageUrl: broadcast.image?.getUrl),
          Spaces.verticalSmall,
          _Title(title: broadcast.title.orEmpty),
          Spaces.verticalMicro,
          _Creator(name: broadcast.hostName.orEmpty),
          Spaces.verticalXLarge,
          MModalListTile(
            leading: const Icon(MIcons.user),
            title: 'Go to Profile',
            onTap: () {
              Navigator.pop(context);
              onGoToProfile();
            },
          ),
          // The creator cannot unsubscribe from themselves, and the API would
          // refuse it. The row is absent rather than present and broken.
          if (!proxy.isHostedByMe) ...[
            Spaces.verticalSmall,
            MModalListTile(
              leading: const Icon(MIcons.user_minus_01),
              title: 'Unsubscribe',
              onTap: () async {
                Navigator.pop(context);
                await onUnsubscribe();
              },
            ),
          ],
          Spaces.verticalSmall,
          MModalListTile(
            leading: const Icon(MIcons.access_time),
            title: proxy.isFavourited
                ? 'Remove from Listen Later'
                : 'Listen Later',
            onTap: () async {
              Navigator.pop(context);
              await onListenLater();
            },
          ),
          Spaces.verticalSmall,
          MModalListTile(
            leading: const Icon(MIcons.share),
            title: 'Share',
            onTap: () async {
              Navigator.pop(context);
              await onShare();
            },
          ),
          Spaces.verticalSmall,
          MModalListTile(
            leading: const Icon(MIcons.link_02),
            title: 'Copy Link',
            onTap: () async {
              Navigator.pop(context);
              await onCopyLink();
            },
          ),
          Spaces.verticalXLarge,
        ],
      ),
    );
  }

  static Future<void> show(
    BuildContext context,
    BroadcastProxy proxy, {
    required VoidCallback onGoToProfile,
    required Future<void> Function() onUnsubscribe,
    required Future<void> Function() onListenLater,
    required Future<void> Function() onShare,
    required Future<void> Function() onCopyLink,
  }) {
    return showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      useRootNavigator: true,
      builder: (context) => _OptionsModal._(
        proxy: proxy,
        onGoToProfile: onGoToProfile,
        onUnsubscribe: onUnsubscribe,
        onListenLater: onListenLater,
        onShare: onShare,
        onCopyLink: onCopyLink,
      ),
    );
  }
}
