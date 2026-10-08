import 'package:flutter/material.dart';
import 'package:flutter/widget_previews.dart';

/// Previews the [MText] widget
@Preview(name: 'My Sample Text')
Widget previewMText() {
  return const MText('Hello, World!');
}

/// A widget that displays styled text with customizable properties.
///
/// This widget provides a convenient way to display text with various
/// styling options and control over behavior.
class MText extends StatelessWidget {
  /// Creates a new `MText` widget.
  ///
  /// * `data`: The text content to be displayed.
  /// * `color`: The color of the text (defaults to the ambient text color).
  /// * `style`: An optional style to apply to the text.
  /// * `maxLines`: The maximum number of lines to display.
  /// * `locale`: The locale for the text (defaults to the ambient locale).
  /// * `overflow`: How to handle text that overflows the available space.
  /// * `textAlign`: How to align the text horizontally.
  /// * `decoration`: The decoration to apply to the text (e.g., underline).
  /// * `decorationColor`: The color of the text decoration.
  /// * `decorationThickness`: The thickness of the text decoration.
  /// * `softWrap`: Whether to allow the text to wrap onto multiple lines.
  const new(
    this.data, {
    super.key,
    this.color,
    this.style,
    this.maxLines,
    this.locale,
    this.overflow,
    this.textAlign,
    this.decoration,
    this.decorationColor,
    this.decorationThickness,
    this.softWrap,
  });

  /// The text content to be displayed.
  final String data;

  /// The color of the text.
  final Color? color;

  /// An optional style to apply to the text.
  ///
  /// If null or if the `inherit` property is true, the text will inherit
  /// styling from the ambient DefaultTextStyle.
  final TextStyle? style;

  /// The maximum number of lines to display before truncation.
  final int? maxLines;

  /// The locale for the text, used for formatting dates, numbers, etc.
  final Locale? locale;

  /// How to handle text that overflows the available space.
  final TextOverflow? overflow;

  /// How to align the text horizontally.
  final TextAlign? textAlign;

  /// The decoration to apply to the text (e.g., underline, strike through).
  final TextDecoration? decoration;

  /// The color of the text decoration.
  final Color? decorationColor;

  /// The thickness of the text decoration.
  final double? decorationThickness;

  /// Whether to allow the text to wrap onto multiple lines.
  final bool? softWrap;

  @override
  Widget build(BuildContext context) {
    final defaultTextStyle = DefaultTextStyle.of(context);
    var effectiveTextStyle = style;
    if (style == null || style!.inherit) {
      effectiveTextStyle = defaultTextStyle.style.merge(style);
    }
    return DefaultTextStyle(
      style: effectiveTextStyle!.copyWith(color: color),
      child: Text(
        data,
        key: key,
        maxLines: maxLines,
        locale: locale,
        overflow: overflow,
        textAlign: textAlign,
        softWrap: softWrap,
      ),
    );
  }
}
