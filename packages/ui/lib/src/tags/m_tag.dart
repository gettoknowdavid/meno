import 'package:flutter/material.dart';
import 'package:meno_ui/src/text/m_text.dart';
import 'package:meno_ui/src/theme/extensions/m_color_scheme.dart';
import 'package:meno_ui/src/theme/extensions/m_text_theme.dart';
import 'package:meno_ui/src/tokens/corners.dart';
import 'package:meno_ui/src/tokens/spacing.dart';

/// A pill-shaped label for a single topic or category.
///
/// The tag resolves its colours, type and spacing from the theme, so it stays
/// consistent across the light and dark schemes. Every tag surface renders the
/// same height, padding and corner radius.
///
/// Example usage:
/// ```dart
/// MTag(title: 'Live');
/// ```
class MTag extends StatelessWidget {
  /// Creates a tag.
  ///
  /// Parameters:
  /// - [title]: The text drawn inside the pill.
  /// - [key]: An optional key to identify the widget.
  /// - [style]: An optional style merged over the theme's caption style. If
  ///   null, the caption medium style is used.
  const new({required this.title, super.key, this.style});

  /// The text drawn inside the pill.
  final String title;

  /// An optional style merged over the theme's caption style.
  final TextStyle? style;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return Container(
      height: 20,
      constraints: const BoxConstraints(minHeight: 20, maxHeight: 20),
      padding: const EdgeInsets.symmetric(horizontal: Insets.sm),
      alignment: Alignment.centerLeft,
      decoration: ShapeDecoration(
        color: colors.inActiveContainer,
        shape: const RoundedRectangleBorder(borderRadius: Corners.xs),
      ),
      child: MText(
        title,
        style: style ?? textTheme.captionMedium,
        color: colors.onInActiveContainer,
      ),
    );
  }
}
