import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/broadcast/manager/live_session_manager.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';
import 'package:meno_mobile/features/broadcast/widgets/live_broadcast_widgets.dart';
import 'package:meno_mobile/features/profile/model/proxies/user_profile_proxy.dart';
import 'package:meno_ui/meno_ui.dart';

class BroadcastInfoModal extends WatchingWidget {
  const new _() : super(key: null);

  static Future<dynamic> show(BuildContext context) {
    return showModalBottomSheet<dynamic>(
      context: context,
      useRootNavigator: true,
      isScrollControlled: true,
      builder: (_) => const BroadcastInfoModal._(),
    );
  }

  @override
  Widget build(BuildContext context) {
    final broadcast = watchValue<LiveSessionManager, Broadcast>(
      (manager) => manager.broadcast,
    );

    final isHost = watchValue<LiveSessionManager, bool>(
      (manager) => manager.isHost,
    );

    final hostProxy = di<LiveSessionManager>().hostProxy;

    return MModal(
      builder: (context) => Column(
        mainAxisSize: .min,
        children: [
          BroadcastArtworkWidget(image: broadcast.image),
          Spaces.verticalSmall,
          BroadcastTitleWidget(
            title: broadcast.title.orEmpty,
            maxLines: 1,
            padding: const .symmetric(horizontal: 40),
          ),
          Spaces.verticalMicro,
          Padding(
            padding: const .symmetric(horizontal: 40),
            child: Row(
              mainAxisAlignment: .center,
              children: [
                BroadcastCreatorWidget(
                  name: broadcast.hostName.orEmpty,
                ),
                Spaces.horizontalSmall,
                const BroadcastStatusWidget(),
              ],
            ),
          ),
          Spaces.verticalXLarge,
          if (!isHost) ...[
            MModalListTile(
              leading: const Icon(MIcons.arrow_narrow_down_left),
              title: 'Minimize Stream',
              onTap: () => context.go(MRoutes.home),
            ),
            _SubscribeTile(proxy: hostProxy),
          ],
          MModalListTile(
            leading: const Icon(MIcons.share),
            title: 'Share',
            onTap: () {},
          ),
          MModalListTile(
            leading: const Icon(MIcons.link_02),
            title: 'Copy Link',
            onTap: () {},
          ),
          Spaces.verticalXLarge,
        ],
      ),
    );
  }
}

class _SubscribeTile extends WatchingWidget {
  const new({required this.proxy});

  final UserProfileProxy proxy;

  @override
  Widget build(BuildContext context) {
    watch(proxy);

    final isRunning = watch(proxy.toggleFollow.isRunning).value;
    final isFollowing = proxy.isFollowing;

    return MModalListTile(
      leading: Icon(isFollowing ? MIcons.user_minus_01 : MIcons.user_check),
      title: isFollowing ? 'Unsubscribe' : 'Subscribe',
      onTap: isRunning ? null : proxy.toggleFollow.run,
    );
  }
}
