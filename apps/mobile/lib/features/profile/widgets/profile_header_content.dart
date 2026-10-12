import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/profile/model/_model.dart';
import 'package:meno_mobile/features/profile/widgets/profile_buttons.dart';
import 'package:meno_ui/meno_ui.dart';
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
              if (profile.bio != null) ...[
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
class UserProfileHeaderContent extends StatelessWidget {
  /// Creates the header for [profile].
  const new({required this.profile, super.key});

  /// The profile being viewed.
  final Profile profile;

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
            if (profile.bio != null) ...[
              ProfileBio(profile.bio!),
              Spaces.verticalLarge,
            ],
            SizedBox(
              height: 32,
              child: Row(
                children: [
                  // Subscribing is the subscriptions feature's to build: the
                  // follower endpoints and the button's states belong to it.
                  // Until then the slot shows the Meno mark rather than a
                  // button that would pretend to have done something.
                  const Expanded(child: SubscribePlaceholder()),
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

/// Where a subscribe button will go.
///
/// A placeholder rather than a disabled control: a button that cannot work yet
/// invites taps and explains nothing.
class SubscribePlaceholder extends StatelessWidget {
  /// Creates the placeholder.
  const new({super.key});

  @override
  Widget build(BuildContext context) =>
      const Center(child: MPlaceholder(dimension: 24));
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
          // The API counts followers and following; the screens have always
          // called them subscribers and subscriptions.
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

    return MReadMoreText(
      bio,
      style: textTheme.captionRegular.copyWith(
        color: colors.onBackgroundVariant,
      ),
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
