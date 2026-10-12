import 'package:flutter/material.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno/_core/value_objects/value_objects.dart';
import 'package:meno/features/profile/profile.dart';
import 'package:meno/features/profile/widgets/_widgets.dart';
import 'package:meno_design_system/meno_design_system.dart';
import 'package:readmore/readmore.dart';

class MyProfileHeaderContent extends StatelessWidget {
  const MyProfileHeaderContent({required this.profile, super.key});

  final Profile profile;

  @override
  Widget build(BuildContext context) {
    return SafeArea(
      bottom: false,
      child: SingleChildScrollView(
        physics: const NeverScrollableScrollPhysics(),
        padding: const .fromLTRB(16, kToolbarHeight, 16, 0),
        child: Column(
          crossAxisAlignment: .start,
          children: [
            Row(
              children: [
                MAvatar(radius: 40, url: profile.image?.getUrl()),
                const SizedBox(width: 24),
                Expanded(child: ProfileStatsWidget(stats: profile.stats)),
              ],
            ),
            Spaces.verticalLarge,
            const AccountUpgradeSectionWidget(),
            Spaces.verticalLarge,
            if (profile.bio != null) ...[
              ProfileBioWidget(bio: profile.bio!),
              Spaces.verticalLarge,
            ],
            SizedBox(
              height: 32,
              child: Row(
                children: [
                  Expanded(child: EditProfileButton(profile: profile)),
                  Spaces.horizontalLarge,
                  Expanded(child: ShareProfileButton(profile: profile)),
                ],
              ),
            ),
            Spaces.verticalLarge,
          ],
        ),
      ),
    );
  }
}

class UserProfileHeaderContent extends WatchingWidget {
  const UserProfileHeaderContent({required this.proxy, super.key});

  final UserProfileProxy proxy;

  @override
  Widget build(BuildContext context) {
    watch(proxy);
    return SafeArea(
      bottom: false,
      child: SingleChildScrollView(
        physics: const NeverScrollableScrollPhysics(),
        padding: const .fromLTRB(16, kToolbarHeight, 16, 0),
        child: Column(
          crossAxisAlignment: .start,
          children: [
            Row(
              children: [
                MAvatar(radius: 40, url: proxy.imageUrl),
                const SizedBox(width: 24),
                Expanded(child: ProfileStatsWidget(stats: proxy.stats)),
              ],
            ),
            Spaces.verticalLarge,

            if (proxy.bio != null) ...[
              ProfileBioWidget(bio: proxy.bio!),
              Spaces.verticalLarge,
            ],

            SizedBox(
              height: 32,
              child: Row(
                children: [
                  Expanded(child: SubscribeButton(proxy: proxy)),
                  Spaces.horizontalLarge,
                  Expanded(child: ShareProfileButton(profile: proxy.profile)),
                ],
              ),
            ),
            Spaces.verticalLarge,
          ],
        ),
      ),
    );
  }
}

class ProfileStatsWidget extends StatelessWidget {
  const ProfileStatsWidget({required this.stats, super.key});

  final UserStats stats;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: 46,
      child: Row(
        children: [
          const SizedBox(width: 2),
          _StatItem(title: 'Broadcasts', count: stats.broadcasts),
          const Spacer(),
          _StatItem(title: 'Subscribers', count: stats.subscribers),
          const Spacer(),
          _StatItem(title: 'Subscriptions', count: stats.subscriptions),
          const SizedBox(width: 2),
        ],
      ),
    );
  }
}

class _StatItem extends StatelessWidget {
  const _StatItem({required this.title, this.count = 0});

  final String title;
  final int? count;

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);
    final colors = MColorScheme.of(context);

    return Column(
      mainAxisAlignment: .center,
      children: [
        MText(count.toString(), style: textTheme.heading3Medium),
        MText(
          title,
          style: textTheme.microMedium,
          color: colors.onBackgroundVariant,
        ),
      ],
    );
  }
}

class ProfileBioWidget extends StatelessWidget {
  const ProfileBioWidget({
    required this.bio,
    this.forceHeight = false,
    super.key,
  });

  final MultiLineString bio;
  final bool forceHeight;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);
    final style = textTheme.captionMedium.copyWith(
      color: colors.onBackgroundVariant,
    );
    return ReadMoreText(
      bio.getOrNull() ?? '',
      style: textTheme.captionRegular,
      trimLines: 3,
      trimMode: TrimMode.Line,
      trimExpandedText: '\nless',
      trimCollapsedText: '\nmore',
      moreStyle: style,
      lessStyle: style,
    );
  }
}

class AccountUpgradeSectionWidget extends StatelessWidget {
  const AccountUpgradeSectionWidget({super.key});

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);
    return SizedBox(
      height: 24,
      child: Row(
        children: [
          const MTag(title: 'FREE ACCOUNT', height: 24),
          Spaces.horizontalLarge,
          MTextButton(
            label: 'Upgrade to Premium',
            onPressed: () {},
            style: TextButton.styleFrom(
              padding: .zero,
              textStyle: textTheme.captionMedium.copyWith(
                decoration: .underline,
              ),
            ),
          ),
        ],
      ),
    );
  }
}
