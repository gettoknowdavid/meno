import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/buttons/m_icon_button.dart';
import 'package:meno_ui/src/text/m_text.dart';
import 'package:meno_ui/src/theme/extensions/m_color_scheme.dart';
import 'package:meno_ui/src/theme/extensions/m_text_theme.dart';
import 'package:meno_ui/src/tokens/m_icons.dart';
import 'package:meno_ui/src/tokens/spacing.dart';

/// The size of the chevron drawn by the back button.
const double _kIconSize = Insets.lg;

/// A "back" button, either as a lone chevron or as a chevron with a label.
///
/// Both variants write to the same callback: pass `onPressed` to handle
/// navigation yourself, or omit it to pop the current route.
///
/// Example usage:
/// ```dart
/// MBackButton(onPressed: Navigator.of(context).pop);
/// ```
///
/// ```dart
/// MBackButton.withText(onPressed: Navigator.of(context).pop);
/// ```
class MBackButton extends StatelessWidget {
  /// Creates an icon-only back button.
  const new({super.key, this.onPressed, this.color})
    : title = null,
      textStyle = null,
      _hasLabel = false;

  /// Creates a back button with a label beside the chevron.
  ///
  /// The label falls back to 'Back' and the colours to the theme's background
  /// foreground. Use [iconColor] to recolour just the button, which is the
  /// colour the icon-only variant calls `color`.
  const new withText({
    super.key,
    this.onPressed,
    String? title,
    Color? iconColor,
    this.textStyle,
  }) : title = title ?? 'Back',
       color = iconColor,
       _hasLabel = true;

  /// Whether the button draws a label beside the chevron.
  final bool _hasLabel;

  /// The label drawn beside the chevron, or null for the icon-only variant.
  final String? title;

  /// The colour of the chevron and label, defaulting to the theme.
  final Color? color;

  /// The style of the label, defaulting to the theme's caption style.
  final TextStyle? textStyle;

  /// The callback invoked when the button is pressed.
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    if (!_hasLabel) {
      return MIconButton(
        onPressed: () => _onPressed(context),
        icon: const Icon(MIcons.chevron_left),
        color: color,
      );
    }

    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);
    final effectiveColor = color ?? colors.onBackground;

    return GestureDetector(
      onTap: () => _onPressed(context),
      child: Row(
        children: [
          Icon(MIcons.chevron_left, color: effectiveColor, size: _kIconSize),
          MText(
            title!,
            style: textStyle ?? textTheme.captionMedium,
            color: effectiveColor,
          ),
        ],
      ),
    );
  }

  /// Runs [onPressed], falling back to popping the current route.
  void _onPressed(BuildContext context) {
    if (onPressed != null) {
      return onPressed!();
    }
    Navigator.maybePop(context);
  }
}
