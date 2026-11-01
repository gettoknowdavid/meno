import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/profile/model/_model.dart';
import 'package:meno_ui/meno_ui.dart';

/// One account, as a search result draws it.
///
/// Presentational on purpose: the entity comes in and a tap goes out. Which
/// account it is, and whether anything should be done about it, is the screen's
/// business — this card only paints the public half of a profile.
///
/// It fills its cell rather than owning a size: the results grid decides how
/// wide a card is, and the same card is drawn at either column count.
class ProfileCard extends StatelessWidget {
  /// Creates a card for [profile], tappable through [onTap].
  const new({required this.profile, required this.onTap, super.key});

  /// The account this card draws.
  final Profile profile;

  /// What a tap opens — usually the account's profile page.
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final styles = MCardStyles.of(context);

    return GestureDetector(
      onTap: onTap,
      child: Container(
        padding: const EdgeInsets.all(Insets.md),
        decoration: ShapeDecoration(
          color: styles.backgroundColor,
          shadows: Shadows.soft,
          shape: Corners.squircleLg,
        ),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            MAvatar(
              radius: 32,
              url: profile.avatarUrl?.getUrl,
              hasBorder: false,
            ),
            Spaces.verticalSmall,
            MText(
              profile.fullName.orEmpty,
              style: styles.titleStyle,
              color: styles.titleColor,
              textAlign: TextAlign.center,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
            ),
            Spaces.verticalMicro,
            Flexible(
              child: MText(
                profile.bio?.orEmpty ?? '',
                style: styles.hostStyle,
                color: styles.hostColor,
                textAlign: TextAlign.center,
                maxLines: 2,
                overflow: TextOverflow.ellipsis,
              ),
            ),
          ],
        ),
      ),
    );
  }
}
