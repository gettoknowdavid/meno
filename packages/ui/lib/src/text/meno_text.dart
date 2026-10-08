import 'package:flutter/material.dart';
import 'package:meno_ui/src/tokens/m_text_styles.dart';

/// Custom thickness of the glyphs used to draw the text.
enum MenoFontWeight {
  /// [FontWeight.normal] or [FontWeight.w400]
  regular,

  /// [FontWeight.w500]
  medium,

  /// [FontWeight.bold] or [FontWeight.w700]
  bold,
}

/// {@template meno_text}
/// A run of text with a single style.
/// {@endtemplate}
class MenoText extends StatelessWidget {
  const new _(
    this.data, {
    super.key,
    this.textAlign,
    this.textDirection,
    this.locale,
    this.softWrap,
    this.overflow,
    this.maxLines,
    this.semanticsLabel,
    this.weight = MenoFontWeight.medium,
    this.style,
    this.decoration,
    this.color,
    this.decorationColor,
  });

  /// Heading 1
  const factory heading1(
    String data, {
    Key? key,
    MenoFontWeight weight,
    Color? color,
    TextAlign? textAlign,
    TextDirection? textDirection,
    Locale? locale,
    TextOverflow? overflow,
    int? maxLines,
    String? semanticsLabel,
    TextDecoration? decoration,
    Color? decorationColor,
    bool? softWrap,
  }) = _Heading1;

  /// Heading 2
  const factory heading2(
    String data, {
    Key? key,
    MenoFontWeight weight,
    Color? color,
    TextAlign? textAlign,
    TextDirection? textDirection,
    Locale? locale,
    TextOverflow? overflow,
    int? maxLines,
    String? semanticsLabel,
    TextDecoration? decoration,
    Color? decorationColor,
    bool? softWrap,
  }) = _Heading2;

  /// Heading 3
  const factory heading3(
    String data, {
    Key? key,
    MenoFontWeight weight,
    Color? color,
    TextAlign? textAlign,
    TextDirection? textDirection,
    Locale? locale,
    TextOverflow? overflow,
    int? maxLines,
    String? semanticsLabel,
    TextDecoration? decoration,
    Color? decorationColor,
    bool? softWrap,
  }) = _Heading3;

  /// Subheading
  const factory subheading(
    String data, {
    Key? key,
    MenoFontWeight weight,
    Color? color,
    TextAlign? textAlign,
    TextDirection? textDirection,
    Locale? locale,
    TextOverflow? overflow,
    int? maxLines,
    String? semanticsLabel,
    TextDecoration? decoration,
    Color? decorationColor,
    bool? softWrap,
  }) = _Subheading;

  /// Body
  const factory body(
    String data, {
    Key? key,
    MenoFontWeight weight,
    Color? color,
    TextAlign? textAlign,
    TextDirection? textDirection,
    Locale? locale,
    TextOverflow? overflow,
    int? maxLines,
    String? semanticsLabel,
    TextDecoration? decoration,
    Color? decorationColor,
    bool? softWrap,
  }) = _Body;

  /// Caption
  const factory caption(
    String data, {
    Key? key,
    MenoFontWeight weight,
    Color? color,
    TextAlign? textAlign,
    TextDirection? textDirection,
    Locale? locale,
    TextOverflow? overflow,
    int? maxLines,
    String? semanticsLabel,
    TextDecoration? decoration,
    Color? decorationColor,
    bool? softWrap,
  }) = _Caption;

  /// Micro
  const factory micro(
    String data, {
    Key? key,
    MenoFontWeight weight,
    Color? color,
    TextAlign? textAlign,
    TextDirection? textDirection,
    Locale? locale,
    TextOverflow? overflow,
    int? maxLines,
    String? semanticsLabel,
    TextDecoration? decoration,
    Color? decorationColor,
    bool? softWrap,
  }) = _Micro;

  /// Nano
  const factory nano(
    String data, {
    Key? key,
    MenoFontWeight weight,
    Color? color,
    TextAlign? textAlign,
    TextDirection? textDirection,
    Locale? locale,
    TextOverflow? overflow,
    int? maxLines,
    String? semanticsLabel,
    TextDecoration? decoration,
    Color? decorationColor,
    bool? softWrap,
  }) = _Nano;

  /// Button
  const factory button(
    String data, {
    Key? key,
    MenoFontWeight weight,
    Color? color,
    TextAlign? textAlign,
    TextDirection? textDirection,
    Locale? locale,
    TextOverflow? overflow,
    int? maxLines,
    String? semanticsLabel,
    TextDecoration? decoration,
    Color? decorationColor,
    bool? softWrap,
  }) = _Button;

  /// The text to display.
  final String data;

  /// How the text should be aligned horizontally.
  final TextAlign? textAlign;

  /// The directionality of the text.
  ///
  /// This decides how [textAlign] values like [TextAlign.start] and
  /// [TextAlign.end] are interpreted.
  ///
  /// This is also used to disambiguate how to render bidirectional text. For
  /// example, if the [data] is an English phrase followed by a Hebrew phrase,
  /// in a [TextDirection.ltr] context the English phrase will be on the left
  /// and the Hebrew phrase to its right, while in a [TextDirection.rtl]
  /// context, the English phrase will be on the right and the Hebrew phrase on
  /// its left.
  ///
  /// Defaults to the ambient [Directionality], if any.
  final TextDirection? textDirection;

  /// Used to select a font when the same Unicode character can
  /// be rendered differently, depending on the locale.
  ///
  /// It's rarely necessary to set this property. By default its value
  /// is inherited from the enclosing app with
  /// `Localizations.localeOf(context)`.
  ///
  final Locale? locale;

  /// Whether the text should break at soft line breaks.
  ///
  /// If false, the glyphs in the text will be positioned as if there was
  /// unlimited horizontal space.
  final bool? softWrap;

  /// How visual overflow should be handled.
  ///
  /// If this is null [TextStyle.overflow] will be used, otherwise the value
  /// from the nearest [DefaultTextStyle] ancestor will be used.
  final TextOverflow? overflow;

  /// An optional maximum number of lines for the text to span, wrapping
  /// if necessary.
  /// If the text exceeds the given number of lines, it will be truncated
  /// according to [overflow].
  ///
  /// If this is 1, text will not wrap. Otherwise, text will be wrapped at the
  /// edge of the box.
  ///
  /// If this is null, but there is an ambient [DefaultTextStyle] that specifies
  /// an explicit number for its [DefaultTextStyle.maxLines], then the
  /// [DefaultTextStyle] value will take precedence. You can use a [RichText]
  /// widget directly to entirely override the [DefaultTextStyle].
  final int? maxLines;

  /// {@template flutter.widgets.Text.semanticsLabel}
  /// An alternative semantics label for this text.
  ///
  /// If present, the semantics of this widget will contain this value instead
  /// of the actual text. This will overwrite any of the semantics labels
  /// applied directly to the [TextSpan]s.
  ///
  /// This is useful for replacing abbreviations or shorthands with the full
  /// text value:
  ///
  /// ```dart
  /// const Text(r'$$', semanticsLabel: 'Double dollars')
  /// ```
  /// {@endtemplate}
  final String? semanticsLabel;

  /// Override of the normal text style
  final TextStyle? style;

  /// Custom font weight for the text
  final MenoFontWeight weight;

  /// Convenient way to set the color of the text without copying the style
  final Color? color;

  /// [TextDecoration]
  final TextDecoration? decoration;

  /// [TextDecoration] color
  final Color? decorationColor;

  @override
  Widget build(BuildContext context) {
    return Text(
      data,
      key: key,
      locale: locale,
      maxLines: maxLines,
      overflow: overflow,
      textAlign: textAlign,
      textDirection: textDirection,
      softWrap: softWrap,
      semanticsLabel: semanticsLabel,
      style: style?.copyWith(
        color: color,
        decoration: decoration,
        decorationColor: decorationColor,
        fontWeight: switch (weight) {
          MenoFontWeight.regular => FontWeight.w400,
          MenoFontWeight.medium => FontWeight.w500,
          MenoFontWeight.bold => FontWeight.bold,
        },
      ),
    );
  }
}

class _Heading1 extends MenoText {
  const new(
    super.data, {
    super.key,
    super.weight,
    super.color,
    super.textAlign,
    super.textDirection,
    super.locale,
    super.overflow,
    super.maxLines,
    super.semanticsLabel,
    super.decoration,
    super.decorationColor,
    super.softWrap,
  }) : super._(style: MTextStyles.heading1);
}

class _Heading2 extends MenoText {
  const new(
    super.data, {
    super.key,
    super.weight,
    super.color,
    super.textAlign,
    super.textDirection,
    super.locale,
    super.overflow,
    super.maxLines,
    super.semanticsLabel,
    super.decoration,
    super.decorationColor,
    super.softWrap,
  }) : super._(style: MTextStyles.heading2);
}

class _Heading3 extends MenoText {
  const new(
    super.data, {
    super.key,
    super.weight,
    super.color,
    super.textAlign,
    super.textDirection,
    super.locale,
    super.overflow,
    super.maxLines,
    super.semanticsLabel,
    super.decoration,
    super.decorationColor,
    super.softWrap,
  }) : super._(style: MTextStyles.heading3);
}

class _Subheading extends MenoText {
  const new(
    super.data, {
    super.key,
    super.weight,
    super.color,
    super.textAlign,
    super.textDirection,
    super.locale,
    super.overflow,
    super.maxLines,
    super.semanticsLabel,
    super.decoration,
    super.decorationColor,
    super.softWrap,
  }) : super._(style: MTextStyles.subheading);
}

class _Body extends MenoText {
  const new(
    super.data, {
    super.key,
    super.weight,
    super.color,
    super.textAlign,
    super.textDirection,
    super.locale,
    super.overflow,
    super.maxLines,
    super.semanticsLabel,
    super.decoration,
    super.decorationColor,
    super.softWrap,
  }) : super._(style: MTextStyles.body);
}

class _Caption extends MenoText {
  const new(
    super.data, {
    super.key,
    super.weight,
    super.color,
    super.textAlign,
    super.textDirection,
    super.locale,
    super.overflow,
    super.maxLines,
    super.semanticsLabel,
    super.decoration,
    super.decorationColor,
    super.softWrap,
  }) : super._(style: MTextStyles.caption);
}

class _Micro extends MenoText {
  const new(
    super.data, {
    super.key,
    super.weight,
    super.color,
    super.textAlign,
    super.textDirection,
    super.locale,
    super.overflow,
    super.maxLines,
    super.semanticsLabel,
    super.decoration,
    super.decorationColor,
    super.softWrap,
  }) : super._(style: MTextStyles.micro);
}

class _Nano extends MenoText {
  const new(
    super.data, {
    super.key,
    super.weight,
    super.color,
    super.textAlign,
    super.textDirection,
    super.locale,
    super.overflow,
    super.maxLines,
    super.semanticsLabel,
    super.decoration,
    super.decorationColor,
    super.softWrap,
  }) : super._(style: MTextStyles.nano);
}

class _Button extends MenoText {
  const new(
    super.data, {
    super.key,
    super.weight,
    super.color,
    super.textAlign,
    super.textDirection,
    super.locale,
    super.overflow,
    super.maxLines,
    super.semanticsLabel,
    super.decoration,
    super.decorationColor,
    super.softWrap,
  }) : super._(style: MTextStyles.button);
}
