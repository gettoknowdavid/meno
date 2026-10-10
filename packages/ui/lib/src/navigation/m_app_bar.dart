//
// ignore_for_file: library_private_types_in_public_api

import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// Enum representing different types of app bars.
enum _AppBarVariant { home, primary, secondary }

/// A factory class for creating different types of app bars.
class MAppBar extends StatelessWidget implements PreferredSizeWidget {
  /// Creates a primary app bar with the given title.
  ///
  /// This app bar typically has a back button and is used for navigation.
  ///
  /// - [title]: The title of the app bar.
  /// - [key]: The key for the widget.
  /// - [backText]: The text for the back button. Defaults to 'Back'.
  /// - [onBackPressed]: A callback function called when the back button is
  /// pressed (optional).
  /// - [implyLeading]: Whether to imply a leading icon. Defaults to true.
  new primary({
    required this.title,
    super.key,
    this.backText = 'Back',
    this.onBackPressed,
    this.implyLeading = true,
    this.centerTitle = false,
    this.actions,
    this.showBottomBorder = false,
    this.avatarImageUrl,
    this.onAvatarTap,
    this.onNotificationBellTap,
  }) : variant = .primary,
       child = _PrimaryAppBarImpl(
         title,
         backText: backText,
         onBackPressed: onBackPressed,
         implyLeading: implyLeading,
       );

  /// Creates a secondary app bar with the given title.
  ///
  /// This app bar is typically used for screens with less emphasis on
  /// navigation.
  ///
  /// - [title]: The title of the app bar.
  /// - [key]: The key for the widget.
  /// - [onBackPressed]: A callback function called when the back button is
  /// pressed (optional).
  /// - [centerTitle]: Whether to center the title. Defaults to false.
  /// - [actions]: A list of widgets to display on the right side of the
  /// app bar (optional).
  /// - [showBottomBorder]: Whether to show a bottom border. Defaults to false.
  new secondary({
    required this.title,
    super.key,
    this.onBackPressed,
    this.centerTitle = false,
    this.actions,
    this.showBottomBorder = false,
    this.backText = 'Back',
    this.implyLeading = true,
    this.avatarImageUrl,
    this.onAvatarTap,
    this.onNotificationBellTap,
  }) : variant = .secondary,
       child = _SecondaryAppBarImpl(
         title,
         onBackPressed: onBackPressed,
         centerTitle: centerTitle,
         actions: actions,
         showBottomBorder: showBottomBorder,
       );

  /// Creates a home app bar with the given title.
  ///
  /// This app bar is typically used for the main screen of an app.
  ///
  /// - [title]: The title of the app bar.
  /// - [key]: The key for the widget.
  /// - [centerTitle]: Whether to center the title. Defaults to false.
  /// - [avatarImageUrl]: The URL of the user's avatar image (optional).
  /// - [onAvatarTap]: A callback function called when the avatar is tapped
  /// (optional).
  /// - [onNotificationBellTap]: A callback function called when the
  /// notification bell is tapped (optional).
  new home({
    required this.title,
    super.key,
    this.centerTitle = false,
    this.avatarImageUrl,
    this.onAvatarTap,
    this.onNotificationBellTap,
    this.backText = 'Back',
    this.implyLeading = true,
    this.actions,
    this.showBottomBorder = false,
    this.onBackPressed,
  }) : variant = .home,
       child = _HomeAppBarImpl(
         title,
         onBackPressed: onBackPressed,
         centerTitle: centerTitle,
         avatarImageUrl: avatarImageUrl,
         onAvatarTap: onAvatarTap,
         onNotificationBellTap: onNotificationBellTap,
       );

  /// The title of the app bar.
  final String title;

  /// A callback function called when the back button is pressed.
  final VoidCallback? onBackPressed;

  /// For primary app bar: Whether to imply a leading icon. Defaults to true.
  final bool implyLeading;

  /// For secondary and home app bars: Whether to center the title. Defaults
  /// to false.
  final bool centerTitle;

  /// For secondary app bar: A list of widgets to display on the right side.
  final List<Widget>? actions;

  /// For secondary app bar: Whether to show a bottom border. Defaults to false.
  final bool showBottomBorder;

  /// For home app bar: The URL of the user's avatar image (optional).
  final String? avatarImageUrl;

  /// For home app bar: Callback when avatar is tapped (optional).
  final VoidCallback? onAvatarTap;

  /// For home app bar: Callback when notification bell is tapped (optional).
  final VoidCallback? onNotificationBellTap;

  /// For primary app bar: The text for the back button. Defaults to 'Back'.
  final String backText;

  /// The content of the app bar.
  final Widget child;

  /// The type of app bar.
  final _AppBarVariant variant;

  @override
  Size get preferredSize => switch (variant) {
    _AppBarVariant.home => const .fromHeight(120),
    _AppBarVariant.primary => const .fromHeight(120),
    _AppBarVariant.secondary => const .fromHeight(kToolbarHeight),
  };

  @override
  Widget build(BuildContext context) => child;
}

class _PrimaryAppBarImpl extends StatelessWidget {
  const new(
    this.title, {
    required this.backText,
    required this.implyLeading,
    this.onBackPressed,
  });

  final String title;
  final String backText;
  final VoidCallback? onBackPressed;
  final bool implyLeading;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);
    final styles = MNavigationStyles.of(context);
    final colorFilter = ColorFilter.mode(styles.accentColor, .srcIn);
    const toolbarHeight = 120.0;

    return AppBar(
      automaticallyImplyLeading: false,
      backgroundColor: colors.primary,
      toolbarHeight: toolbarHeight,
      flexibleSpace: SafeArea(
        child: Stack(
          fit: .passthrough,
          alignment: .centerLeft,
          children: [
            Padding(
              padding: const .symmetric(horizontal: Insets.lg),
              child: Column(
                mainAxisAlignment: .center,
                crossAxisAlignment: .start,
                children: [
                  if (implyLeading) ...[
                    MBackButton.withText(
                      title: backText,
                      iconColor: colors.onPrimary,
                      textStyle: styles.actionTextStyle,
                    ),
                    Spaces.verticalLarge,
                  ],
                  MText(
                    title,
                    style: textTheme.heading2Bold,
                    color: colors.onPrimary,
                  ),
                ],
              ),
            ),
            Positioned(
              right: 0,
              bottom: 0,
              child: Assets.images.geometricLines.svg(
                colorFilter: colorFilter,
                package: 'meno_ui',
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _SecondaryAppBarImpl extends StatelessWidget {
  const new(
    this.title, {
    this.onBackPressed,
    this.centerTitle = false,
    this.actions,
    this.showBottomBorder = false,
  });

  final String title;
  final VoidCallback? onBackPressed;
  final bool centerTitle;
  final List<Widget>? actions;
  final bool showBottomBorder;

  @override
  Widget build(BuildContext context) {
    return AppBar(
      leading: const MBackButton(),
      centerTitle: centerTitle,
      bottom: !showBottomBorder
          ? null
          : const PreferredSize(
              preferredSize: .fromHeight(1),
              child: ColoredBox(
                color: Colors.transparent,
                child: SizedBox(height: 1, width: double.infinity),
              ),
            ),
      title: MText(
        title,
        overflow: .ellipsis,
        maxLines: 1,
        color: MColorScheme.of(context).onBackground,
      ),
      actions: actions,
    );
  }
}

class _HomeAppBarImpl extends StatelessWidget {
  const new(
    this.title, {
    this.onBackPressed,
    this.centerTitle = false,
    this.avatarImageUrl,
    this.onAvatarTap,
    this.onNotificationBellTap,
  });

  final String title;
  final VoidCallback? onBackPressed;
  final bool centerTitle;
  final String? avatarImageUrl;
  final VoidCallback? onAvatarTap;
  final VoidCallback? onNotificationBellTap;

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);
    return AppBar(
      flexibleSpace: SafeArea(
        child: Container(
          margin: const .only(top: Insets.sm),
          padding: const .symmetric(horizontal: 16, vertical: 6),
          height: 56,
          child: Row(
            children: [
              Expanded(
                child: Column(
                  mainAxisAlignment: .end,
                  crossAxisAlignment: .start,
                  mainAxisSize: .min,
                  children: [
                    MText('Hello,', style: textTheme.captionRegular),
                    MText(title, style: textTheme.subheadingMedium),
                  ],
                ),
              ),
              Spaces.horizontalSmall,
              Row(
                children: [
                  MIconButton(
                    icon: const Icon(MIcons.bell),
                    iconSize: 20,
                    onPressed: onNotificationBellTap,
                  ),
                  const SizedBox(width: 24),
                  MAvatar(
                    radius: Insets.lg,
                    url: avatarImageUrl,
                    onTap: onAvatarTap,
                    hasBorder: false,
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }
}
