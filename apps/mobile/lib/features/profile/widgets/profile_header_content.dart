import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/profile/model/_model.dart';
import 'package:meno_mobile/features/profile/widgets/profile_buttons.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:readmore/readmore.dart';
import 'package:skeletonizer/skeletonizer.dart';

/// The header of the signed-in account's own profile: avatar, counters, the
/// upgrade row, the bio, and the two buttons that act on it.
///
/// It is the flexible space of the profile screen's app bar, so it is laid out
/// above the toolbar's height and scrolls away with the tab bar.
class MyProfileHeaderContent extends StatelessWidget {
  /// Creates the header for [profile].
  const new({required this.profile, this.isSkeleton = false, super.key});

  /// The signed-in account's profile, or [MyProfile.empty] while it loads.
  final MyProfile profile;

  /// Whether the record is still on its way.
  ///
  /// True draws the header as a shimmering skeleton — the same layout, with
  /// the design system's bones instead of placeholders that would read as
  /// real numbers.
  final bool isSkeleton;

  @override
  Widget build(BuildContext context) {
    return Skeletonizer(
      enabled: isSkeleton,
      child: SafeArea(
        bottom: false,
        child: SingleChildScrollView(
          physics: const NeverScrollableScrollPhysics(),
          padding: const .fromLTRB(Insets.lg, kToolbarHeight, Insets.lg, 0),
          child: Column(
            crossAxisAlignment: .start,
            children: [
              Row(
                children: [
                  MAvatar(radius: 40, url: profile.avatarUrl),
                  const SizedBox(width: Insets.xl),
                  Expanded(child: ProfileStatsRow(stats: profile.stats)),
                ],
              ),
              Spaces.verticalLarge,
              const AccountUpgradeSection(),
              Spaces.verticalLarge,
              if (profile.bio != null && profile.bio!.isNotEmpty) ...[
                ProfileBio(profile.bio!),
                Spaces.verticalLarge,
              ],
              SizedBox(
                height: 32,
                child: Row(
                  children: [
                    Expanded(child: EditProfileButton(profile: profile)),
                    Spaces.horizontalLarge,
                    Expanded(
                      child: ShareProfileButton(profile: profile.asProfile),
                    ),
                  ],
                ),
              ),
              Spaces.verticalLarge,
            ],
          ),
        ),
      ),
    );
  }
}

/// The header of somebody else's profile: avatar, counters, bio and the two
/// actions a visitor has.
///
/// It takes the [UserProfileProxy] rather than the entity because two of the
/// things it draws are the viewer's, not the account's: whether they follow
/// them, and a follower count that moves with that decision.
class UserProfileHeaderContent extends StatelessWidget {
  /// Creates the header for [profile].
  const new({required this.profile, super.key});

  /// The profile being viewed, as a live proxy.
  final UserProfileProxy profile;

  @override
  Widget build(BuildContext context) {
    return SafeArea(
      bottom: false,
      child: SingleChildScrollView(
        physics: const NeverScrollableScrollPhysics(),
        padding: const .fromLTRB(Insets.lg, kToolbarHeight, Insets.lg, 0),
        child: Column(
          crossAxisAlignment: .start,
          children: [
            Row(
              children: [
                MAvatar(radius: 40, url: profile.avatarUrl),
                const SizedBox(width: Insets.xl),
                Expanded(child: ProfileStatsRow(stats: profile.stats)),
              ],
            ),
            Spaces.verticalLarge,
            if (profile.bio != null && profile.bio!.isNotEmpty) ...[
              ProfileBio(profile.bio!),
              Spaces.verticalLarge,
            ],
            SizedBox(
              height: 32,
              child: Row(
                children: [
                  Expanded(child: SubscribeButton(profile: profile)),
                  Spaces.horizontalLarge,
                  Expanded(child: ShareProfileButton(profile: profile.target)),
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

/// The three counters, side by side.
class ProfileStatsRow extends StatelessWidget {
  /// Creates the row for [stats].
  const new({required this.stats, super.key});

  /// The counters to show.
  final ProfileStats stats;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: 46,
      child: Row(
        children: [
          const SizedBox(width: 2),
          _StatItem(title: 'Broadcasts', count: stats.broadcasts),
          const Spacer(),
          _StatItem(title: 'Subscribers', count: stats.followers),
          const Spacer(),
          _StatItem(title: 'Subscriptions', count: stats.following),
          const SizedBox(width: 2),
        ],
      ),
    );
  }
}

/// One counter: its number above, its name below.
class _StatItem extends StatelessWidget {
  const new({required this.title, required this.count});

  final String title;
  final int count;

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

/// A profile's biography, trimmed to three lines with a way to read the rest.
class ProfileBio extends StatelessWidget {
  /// Creates the bio for [bio].
  const new(this.bio, {super.key});

  /// The text to show.
  final String bio;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);
    final style = textTheme.captionMedium.copyWith(
      color: colors.onBackgroundVariant,
    );
    return ReadMoreText(
      bio,
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

/// The plan the account is on, and the way up from it.
///
/// The row the layout has always had. Nothing behind it yet: upgrading is the
/// subscriptions feature's, so the button is inert rather than a fake request.
class AccountUpgradeSection extends StatelessWidget {
  /// Creates the upgrade row.
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);

    return SizedBox(
      height: 24,
      child: Row(
        children: [
          const MTag(title: 'FREE ACCOUNT'),
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
