import 'package:flutter/material.dart';
import 'package:meno_ui/src/text/m_text.dart';
import 'package:meno_ui/src/theme/extensions/m_color_scheme.dart';
import 'package:meno_ui/src/theme/extensions/m_text_theme.dart';
import 'package:meno_ui/src/tokens/corners.dart';
import 'package:meno_ui/src/tokens/m_icons.dart';
import 'package:meno_ui/src/tokens/spacing.dart';

/// The height of the pill, which sits between the `sm` and `lg` steps of the
/// spacing scale.
const double _kHeight = 34;

/// The width of the pill, which fits the icon, the gap and the label.
const double _kWidth = 94;

/// The size of the icon drawn inside the pill.
const double _kIconSize = Insets.lg;

/// A pill-shaped "Expand" action that reveals more of whatever it labels.
///
/// Example usage:
/// ```dart
/// ExpandButton(
///   onTap: () {
///     // Handle the expand action.
///   },
/// );
/// ```
class ExpandButton extends StatelessWidget {
  /// Creates an expand button.
  ///
  /// Parameters:
  /// - [key]: An optional key to identify the widget.
  /// - [onTap]: The callback invoked when the pill is tapped.
  const new({super.key, this.onTap});

  /// The callback invoked when the pill is tapped.
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return InkWell(
      onTap: onTap,
      borderRadius: Corners.circle,
      child: Container(
        height: _kHeight,
        width: _kWidth,
        padding: const EdgeInsets.symmetric(
          horizontal: Insets.md,
          vertical: Insets.sm,
        ),
        decoration: BoxDecoration(
          color: colors.inActiveContainer,
          borderRadius: Corners.circle,
        ),
        child: Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            Icon(
              MIcons.expand_01,
              size: _kIconSize,
              color: colors.onInActiveContainer,
            ),
            MText(
              'Expand',
              style: textTheme.captionMedium,
              color: colors.onInActiveContainer,
            ),
          ],
        ),
      ),
    );
  }
}
