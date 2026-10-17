import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/broadcast/model/entities.dart';
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
      return UserProfileProxy(profile);
    });

    final textTheme = MTextTheme.of(context);

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
}
