import 'package:material_ui/material_ui.dart';
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
  const new({
    required this.title,
    super.key,
    this.style,
    this.height,
    this.width,
    this.constraints = const BoxConstraints(minHeight: 20, maxHeight: 20),
    this.padding = const .symmetric(horizontal: Insets.sm),
    this.alignment = .centerLeft,
    this.decoration,
    this.textColor,
  });

  /// The text drawn inside the pill.
  final String title;

  /// An optional style merged over the theme's caption style.
  final TextStyle? style;

  /// The height of the tag.
  final double? height;

  /// The width of the tag.
  final double? width;

  /// The constraints of the tag.
  final BoxConstraints? constraints;

  /// The padding of the tag.
  final EdgeInsets? padding;

  /// The alignment of the tag.
  final Alignment? alignment;

  /// The decoration of the tag.
  final Decoration? decoration;

  /// The text color of the tag.
  final Color? textColor;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    final effectiveTextStyle = (style ?? textTheme.nanoRegular).copyWith(
      height: 1,
      color: textColor ?? colors.onInActiveContainer,
    );

    return Container(
      height: height ?? 20,
      constraints: constraints,
      width: width,
      padding: padding,
      alignment: alignment,
      decoration:
          decoration ??
          ShapeDecoration(
            color: colors.inActiveContainer,
            shape: const RoundedRectangleBorder(borderRadius: Corners.xs),
          ),
      child: MText(title, style: effectiveTextStyle),
    );
  }
}
