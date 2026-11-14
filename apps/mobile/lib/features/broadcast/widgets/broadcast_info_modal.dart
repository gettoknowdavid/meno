import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
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
    final snapshot = watchValue<ActiveBroadcastManager, LiveSnapshot>(
      (manager) => manager.snapshot,
    );

    final broadcast = snapshot.broadcast;
    final isCreator = snapshot.canEndBroadcast;

    return MModal(
      builder: (context) => Column(
        mainAxisSize: .min,
        children: [
          BroadcastArtworkWidget(image: broadcast?.image),
          Spaces.verticalSmall,
          BroadcastTitleWidget(
            title: broadcast?.title.orEmpty ?? '',
            maxLines: 1,
            padding: const .symmetric(horizontal: 40),
          ),
          Spaces.verticalMicro,
          Padding(
            padding: const .symmetric(horizontal: 40),
            child: Row(
              mainAxisAlignment: .center,
              children: [
                BroadcastCreatorWidget(name: broadcast?.hostName.orEmpty ?? ''),
                Spaces.horizontalSmall,
                const BroadcastStatusWidget(),
              ],
            ),
          ),
          Spaces.verticalXLarge,
          if (!isCreator)
            MModalListTile(
              leading: const Icon(MIcons.arrow_narrow_down_left),
              title: 'Minimize Stream',
              onTap: () => context.go(MRoutes.home),
            ),
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
