import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/broadcast/manager/active_broadcast_manager.dart';
import 'package:meno_mobile/features/broadcast/model/entities.dart';
import 'package:meno_mobile/features/broadcast/model/live_snapshot.dart';
import 'package:meno_mobile/features/broadcast/services/broadcast_http_service.dart';
import 'package:meno_mobile/features/broadcast/widgets/remove_cohost_dialog.dart';
import 'package:meno_mobile/features/profile/model/entities.dart';
import 'package:meno_mobile/features/profile/model/proxies/user_profile_proxy.dart';
import 'package:meno_mobile/features/profile/widgets/_widgets.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:skeletonizer/skeletonizer.dart';

class ParticipantInfoModal extends WatchingWidget {
  const new _({required this.participant}) : super(key: null);

  final Participant participant;

  static Future<dynamic> show(BuildContext context, Participant participant) {
    return showModalBottomSheet<dynamic>(
      context: context,
      builder: (context) => ParticipantInfoModal._(participant: participant),
      isScrollControlled: true,
      useRootNavigator: true,
    );
  }

  @override
  Widget build(BuildContext context) {
    final proxy = createOnce(() {
      final profile = Profile(
        id: participant.id,
        fullName: participant.fullName,
        bio: participant.bio,
        avatarUrl: participant.avatar,
      );
      return UserProfileProxy(profile.id, profile: profile);
    });

    final textTheme = MTextTheme.of(context);

    // Only the creator may manage cohosts. The snapshot is the authority, so
    // the action disappears as soon as the room is no longer theirs.
    final snapshot = watchValue<ActiveBroadcastManager, LiveSnapshot>(
      (manager) => manager.snapshot,
    );
    final canManage = snapshot.canEndBroadcast;

    return MModal(
      builder: (ctx) => Column(
        mainAxisSize: .min,
        crossAxisAlignment: .stretch,
        children: [
          MAvatar(radius: 36, url: participant.avatar?.getUrl),
          Spaces.verticalLarge,
          Padding(
            padding: const .symmetric(horizontal: 40),
            child: Row(
              mainAxisAlignment: .center,
              mainAxisSize: .min,
              spacing: Insets.sm,
              children: [
                MText(
                  participant.fullName.orEmpty,
                  style: textTheme.heading3Medium,
                  maxLines: 1,
                  overflow: .ellipsis,
                ),
                switch (participant.role) {
                  .host => MBadge.host(ctx),
                  .cohost => MBadge.cohost(ctx),
                  _ => const SizedBox.shrink(),
                },
              ],
            ),
          ),
          Spaces.verticalMicro,
          if (participant.bio != null) ...[
            MText(
              participant.bio?.getOrNull ?? BoneMock.paragraph,
              style: textTheme.subheadingRegular,
              textAlign: .center,
              maxLines: 2,
              overflow: .ellipsis,
            ),
            Spaces.verticalLarge,
          ] else
            Spaces.verticalLarge,
          SubscribeButton(profile: proxy, showIcon: true),
          if (canManage && participant.isCohost) ...[
            Spaces.verticalLarge,
            MDangerButton(
              label: 'Remove co-host',
              onPressed: () => _removeCohost(ctx),
            ),
          ],
          Spaces.verticalSmall,
          MTextButton(
            label: 'View account',
            onPressed: () async {
              await ctx.push(MRoutes.userProfileRoute(participant.id));
            },
          ),
        ],
      ),
    );
  }

  /// Confirms the removal, calls the API and refreshes the roster.
  ///
  /// The broadcast id is captured before the dialog so a removal answered
  /// after an account or room switch is refused rather than applied to the
  /// wrong broadcast.
  Future<void> _removeCohost(BuildContext context) async {
    final live = di<ActiveBroadcastManager>();
    final broadcastId = live.snapshot.value.broadcast?.id;
    if (broadcastId == null || !broadcastId.isValid) return;

    final choice = await RemoveCohostDialog.show(
      context,
      participant.fullName.orEmpty,
    );
    if (choice == null || !context.mounted) return;
    if (!di.isRegistered<ActiveBroadcastManager>()) return;

    final current = di<ActiveBroadcastManager>();
    if (current.snapshot.value.broadcast?.id != broadcastId) return;

    try {
      await di<BroadcastHttpService>().removeCohost(
        broadcastId,
        participant.id,
        removeFromRoom: choice == RemoveCohostChoice.removeFromRoom,
      );

      // The socket refreshes too, but this device must not wait for a frame to
      // stop drawing the removed cohost as one.
      await current.session?.participants?.refresh.runAsync();
      if (!context.mounted) return;

      context.showSuccessToast('Co-host removed');
      Navigator.pop(context);
    } on Object catch (error) {
      if (context.mounted) context.showErrorToast(errorMessage(error));
    }
  }
}
