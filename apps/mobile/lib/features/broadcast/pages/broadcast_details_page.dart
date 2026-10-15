import 'package:flutter/material.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/auth/manager/user_manager.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_ui/meno_ui.dart';

/// One broadcast's own screen: what it is, who made it, and what it says.
///
/// The manager is built here rather than resolved from the locator because a
/// details screen is about one broadcast id, and the id is only known to the
/// route that opened it. [createOnce] gives it that screen's lifetime.
class BroadcastDetailsPage extends WatchingWidget {
  const new({required this.broadcastId, super.key});

  final String broadcastId;

  @override
  Widget build(BuildContext context) {
    final manager = createOnce(
      () => BroadcastDetailsManager(
        http: di<BroadcastHttpService>(),
        userId: di<UserManager>().currentUserId.value ?? .empty,
        broadcastId: Id.fromString(broadcastId),
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
        title: proxy.title.getOrElse((e) => ''),
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
            onPressed: () => _OptionsModal.show(context, proxy.broadcast),
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
            _Title(title: proxy.title.getOrElse((e) => '')),
            Spaces.verticalMicro,
            _Creator(
              name: proxy.creatorName.getOrElse((e) => ''),
              onPressed: () =>
                  context.push(MRoutes.userProfileRoute(proxy.id.getOrCrash)),
            ),
            Spaces.verticalLarge,
            MPrimaryButton.icon(
              label: 'Restream',
              icon: const Icon(MIcons.play),
              onPressed: () {},
            ),
            Spaces.verticalLarge,
            const MDivider(),
            Spaces.verticalXLarge,
            _Description(description: proxy.description.getOrElse((e) => '')),
          ],
        ),
      ),
    );
  }
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
  const new _(this.broadcast) : super(key: null);
  final Broadcast broadcast;

  @override
  Widget build(BuildContext context) {
    return MModal(
      builder: (context) => Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          _Artwork(imageUrl: broadcast.image?.getUrl),
          Spaces.verticalSmall,
          _Title(title: broadcast.title.getOrElse((e) => '')),
          Spaces.verticalMicro,
          _Creator(name: broadcast.hostName.getOrElse((e) => '')),
          Spaces.verticalXLarge,
          MModalListTile(
            leading: const Icon(MIcons.user),
            title: 'Go to Profile',
            onTap: () {
              // TODO(gettoknowdavid): Handle navigation to user's profile
            },
          ),
          Spaces.verticalSmall,
          MModalListTile(
            leading: const Icon(MIcons.user_minus_01),
            title: 'Unsubscribe',
            onTap: () {
              // TODO(gettoknowdavid): Handle unsubscribe
            },
          ),
          Spaces.verticalSmall,
          MModalListTile(
            leading: const Icon(MIcons.access_time),
            title: 'Listen Later',
            onTap: () {
              // TODO(gettoknowdavid): Handle listen later
            },
          ),
          Spaces.verticalSmall,
          MModalListTile(
            leading: const Icon(MIcons.share),
            title: 'Share',
            onTap: () {
              // TODO(gettoknowdavid): Handle share
            },
          ),
          Spaces.verticalSmall,
          MModalListTile(
            leading: const Icon(MIcons.link_02),
            title: 'Copy Link',
            onTap: () {
              // TODO(gettoknowdavid): Handle share link
            },
          ),
          Spaces.verticalXLarge,
        ],
      ),
    );
  }

  static Future<dynamic> show(BuildContext context, Broadcast broadcast) {
    return showModalBottomSheet<dynamic>(
      context: context,
      isScrollControlled: true,
      useRootNavigator: true,
      builder: (context) => _OptionsModal._(broadcast),
    );
  }
}
