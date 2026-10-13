import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/dividers/m_divider.dart';
import 'package:meno_ui/src/text/m_text.dart';
import 'package:meno_ui/src/theme/extensions/m_color_scheme.dart';
import 'package:meno_ui/src/theme/extensions/m_text_theme.dart';
import 'package:meno_ui/src/tokens/m_icons.dart';
import 'package:meno_ui/src/tokens/spacing.dart';

/// Dims [color] while the tile is disabled.
Color? _disabled(Color? color) => color?.withValues(alpha: 0.5);

/// A single row of a settings group.
///
/// Shows a leading icon, a title, an optional trailing widget and a divider,
/// and dims everything when [isDisabled] is true. Compose rows with an
/// `MSettingsSection` to build a settings screen.
///
/// Example usage:
/// ```dart
/// MSettingsListTile(
///   title: 'Notifications',
///   leadingIcon: MIcons.bell,
///   onTap: openNotifications,
/// )
/// ```
class MSettingsListTile extends StatelessWidget {
  /// Creates a settings row.
  ///
  /// - [title]: The row's label.
  /// - [leadingIcon]: An optional glyph before the label.
  /// - [onTap]: Called when the row is tapped; ignored while [isDisabled].
  /// - [isDisabled]: Dims the row and ignores taps.
  /// - [trailing]: The widget after the label, defaulting to a chevron.
  /// - [showDivider]: Whether to draw a divider below the row.
  /// - [titleColor] and [iconColor]: Override the themed colours.
  const new({
    required this.title,
    super.key,
    this.leadingIcon,
    this.onTap,
    this.isDisabled = false,
    this.trailing,
    this.showDivider = true,
    this.titleColor,
    this.iconColor,
  });

  /// The row's label.
  final String title;

  /// An optional glyph shown before the label.
  final IconData? leadingIcon;

  /// Whether the row ignores taps.
  final bool isDisabled;

  /// The widget shown after the label.
  final Widget? trailing;

  /// Called when the row is tapped.
  final VoidCallback? onTap;

  /// Whether to draw a divider below the row.
  final bool showDivider;

  /// The label colour, overriding the disabled/default colour.
  final Color? titleColor;

  /// The glyph colour, overriding the disabled/default colour.
  final Color? iconColor;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    final effectiveTextColor =
        titleColor ??
        (isDisabled ? colors.onInActive : colors.onBackground);

    return Column(
      children: [
        ListTile(
          tileColor: isDisabled
              ? _disabled(colors.surfaceTint)
              : colors.surfaceTint,
          onTap: isDisabled ? null : onTap,
          minTileHeight: 56,
          leading: leadingIcon == null
              ? null
              : Icon(
                  leadingIcon,
                  size: 20,
                  color: iconColor ??
                      (isDisabled ? _disabled(colors.primary) : colors.primary),
                ),
          horizontalTitleGap: Insets.sm,
          contentPadding: const .symmetric(horizontal: Insets.lg),
          title: MText(
            title,
            style: textTheme.captionMedium,
            color: effectiveTextColor,
          ),
          trailing:
              trailing ??
              Icon(
                MIcons.chevron_right,
                size: 20,
                color: isDisabled
                    ? _disabled(colors.onBackgroundVariant)
                    : colors.onBackgroundVariant,
              ),
        ),
        if (showDivider) const MDivider(),
      ],
    );
  }
}
