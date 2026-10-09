import 'package:flutter/material.dart';
import 'package:flutter/widget_previews.dart';
import 'package:meno_ui/src/theme/extensions/m_text_theme.dart';
import 'package:meno_ui/src/theme/m_theme.dart';
import 'package:meno_ui/src/tokens/spacing.dart';

/// Previews plain [MText], which inherits the ambient [DefaultTextStyle].
///
/// Deliberately bare: plain text is a `Text` with no theme involvement, so
/// there is nothing for an app wrapper to supply.
@Preview(name: 'My Sample Text', group: 'MText')
Widget previewMText() {
  return const MText('Hello, World!');
}

/// Previews every [MText] scale variant, largest first.
///
/// The variants resolve their style from [MTextTheme], so this preview supplies
/// [MTheme] — unlike [previewMText], which needs nothing.
@Preview(name: 'Type scale', group: 'MText', size: Size(480, 560))
Widget previewMTextScale() {
  return MaterialApp(
    theme: MTheme.light,
    home: const Scaffold(
      body: Padding(
        padding: EdgeInsets.all(Insets.lg),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            MText.heading1('Heading 1'),
            MText.heading2('Heading 2'),
            MText.heading3('Heading 3'),
            MText.subheading('Subheading'),
            MText.body('Body'),
            MText.caption('Caption'),
            MText.micro('Micro'),
            MText.nano('Nano'),
            MText.button('Button'),
          ],
        ),
      ),
    ),
  );
}

/// Previews [MText]'s three [MFontWeight]s on one variant.
@Preview(name: 'Weights', group: 'MText', size: Size(480, 200))
Widget previewMTextWeights() {
  return MaterialApp(
    theme: MTheme.light,
    home: const Scaffold(
      body: Padding(
        padding: EdgeInsets.all(Insets.lg),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            MText.body('Regular', weight: MFontWeight.regular),
            MText.body('Medium', weight: MFontWeight.medium),
            MText.body('Bold', weight: MFontWeight.bold),
          ],
        ),
      ),
    ),
  );
}

/// Previews the style overrides plain [MText] applies over the ambient style.
@Preview(name: 'Overrides', group: 'MText', size: Size(480, 240))
Widget previewMTextOverrides() {
  return MaterialApp(
    theme: MTheme.light,
    home: const Scaffold(
      body: Padding(
        padding: EdgeInsets.all(Insets.lg),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            MText(
              'Underlined',
              decoration: TextDecoration.underline,
              decorationThickness: 2,
            ),
            MText('Struck through', decoration: TextDecoration.lineThrough),
            MText('Custom colour', color: Colors.pink),
            MText.rich(
              TextSpan(
                text: 'Rich text with ',
                children: [TextSpan(text: 'a span')],
              ),
            ),
          ],
        ),
      ),
    ),
  );
}

/// The weight applied to an [MText]'s glyphs.
///
/// Three steps, matching the three weights [MTextTheme] publishes for every
/// step of the scale, so a weight always resolves to a style the theme
/// actually defines rather than a synthesised `FontWeight`.
enum MFontWeight {
  /// The theme's regular style (w500).
  regular,

  /// The theme's medium style (w600).
  medium,

  /// The theme's bold style (w700).
  bold,
}

/// The type-scale step a named [MText] constructor renders at.
///
/// Private: callers pick a step with a named constructor
/// ([MText.heading1], [MText.body], …), never by naming the step itself.
enum _Scale {
  heading1,
  heading2,
  heading3,
  subheading,
  body,
  caption,
  micro,
  nano,
  button,
}

/// A run of text with a single style.
///
/// This is the only text widget the design system exposes: the plain
/// constructor mirrors [Text] so it can replace it everywhere, and the named
/// constructors render one step of the type scale.
///
/// # Plain text
///
/// ```dart
/// const MText('Hello, World!');
/// ```
///
/// Behaves like [Text]: it inherits the ambient [DefaultTextStyle], honours a
/// [style] that opts out of that inheritance, and accepts every layout
/// parameter [Text] does. When no override is supplied the ambient style is
/// passed through untouched rather than copied.
///
/// # The type scale
///
/// ```dart
/// const MText.heading1('Meno');
/// const MText.body('Body copy');
/// ```
///
/// A scale variant resolves its style from [MTextTheme], not from the
/// `MTextStyles` primitives — which is what makes it brightness-aware and lets
/// the app restyle the scale once. It therefore needs an [MTheme] ancestor and
/// throws without one. [weight] selects the theme's regular, medium or bold
/// style for that step; left unset, the step's regular style is used, so a
/// variant renders at the weight the scale declares rather than at the widget's
/// own default.
///
/// # Overrides
///
/// [color], [decoration], [decorationColor], [decorationStyle] and
/// [decorationThickness] are applied on top of whichever base style was
/// resolved, for both plain text and scale variants. A null override leaves the
/// base style's own value alone.
///
/// See also:
///   - [MText.rich], for text with multiple styles.
///   - [MTextTheme], which publishes the scale.
class MText extends StatelessWidget {
  /// Creates a run of text that inherits the ambient [DefaultTextStyle].
  const new(
    String this.data, {
    super.key,
    this.style,
    this.color,
    this.weight,
    this.decoration,
    this.decorationColor,
    this.decorationStyle,
    this.decorationThickness,
    this.strutStyle,
    this.textAlign,
    this.textDirection,
    this.locale,
    this.softWrap,
    this.overflow,
    this.textScaler,
    this.maxLines,
    this.semanticsLabel,
    this.semanticsIdentifier,
    this.textWidthBasis,
    this.textHeightBehavior,
    this.selectionColor,
  }) : textSpan = null,
       _variant = null;

  /// Creates a run of text from an [InlineSpan], for text with multiple styles.
  const new rich(
    InlineSpan this.textSpan, {
    super.key,
    this.style,
    this.color,
    this.weight,
    this.decoration,
    this.decorationColor,
    this.decorationStyle,
    this.decorationThickness,
    this.strutStyle,
    this.textAlign,
    this.textDirection,
    this.locale,
    this.softWrap,
    this.overflow,
    this.textScaler,
    this.maxLines,
    this.semanticsLabel,
    this.semanticsIdentifier,
    this.textWidthBasis,
    this.textHeightBehavior,
    this.selectionColor,
  }) : data = null,
       _variant = null;

  /// Creates text at the `heading1` step of the type scale.
  const new heading1(
    String this.data, {
    super.key,
    this.style,
    this.color,
    this.weight,
    this.decoration,
    this.decorationColor,
    this.decorationStyle,
    this.decorationThickness,
    this.strutStyle,
    this.textAlign,
    this.textDirection,
    this.locale,
    this.softWrap,
    this.overflow,
    this.textScaler,
    this.maxLines,
    this.semanticsLabel,
    this.semanticsIdentifier,
    this.textWidthBasis,
    this.textHeightBehavior,
    this.selectionColor,
  }) : textSpan = null,
       _variant = _Scale.heading1;

  /// Creates text at the `heading2` step of the type scale.
  const new heading2(
    String this.data, {
    super.key,
    this.style,
    this.color,
    this.weight,
    this.decoration,
    this.decorationColor,
    this.decorationStyle,
    this.decorationThickness,
    this.strutStyle,
    this.textAlign,
    this.textDirection,
    this.locale,
    this.softWrap,
    this.overflow,
    this.textScaler,
    this.maxLines,
    this.semanticsLabel,
    this.semanticsIdentifier,
    this.textWidthBasis,
    this.textHeightBehavior,
    this.selectionColor,
  }) : textSpan = null,
       _variant = _Scale.heading2;

  /// Creates text at the `heading3` step of the type scale.
  const new heading3(
    String this.data, {
    super.key,
    this.style,
    this.color,
    this.weight,
    this.decoration,
    this.decorationColor,
    this.decorationStyle,
    this.decorationThickness,
    this.strutStyle,
    this.textAlign,
    this.textDirection,
    this.locale,
    this.softWrap,
    this.overflow,
    this.textScaler,
    this.maxLines,
    this.semanticsLabel,
    this.semanticsIdentifier,
    this.textWidthBasis,
    this.textHeightBehavior,
    this.selectionColor,
  }) : textSpan = null,
       _variant = _Scale.heading3;

  /// Creates text at the `subheading` step of the type scale.
  const new subheading(
    String this.data, {
    super.key,
    this.style,
    this.color,
    this.weight,
    this.decoration,
    this.decorationColor,
    this.decorationStyle,
    this.decorationThickness,
    this.strutStyle,
    this.textAlign,
    this.textDirection,
    this.locale,
    this.softWrap,
    this.overflow,
    this.textScaler,
    this.maxLines,
    this.semanticsLabel,
    this.semanticsIdentifier,
    this.textWidthBasis,
    this.textHeightBehavior,
    this.selectionColor,
  }) : textSpan = null,
       _variant = _Scale.subheading;

  /// Creates text at the `body` step of the type scale.
  const new body(
    String this.data, {
    super.key,
    this.style,
    this.color,
    this.weight,
    this.decoration,
    this.decorationColor,
    this.decorationStyle,
    this.decorationThickness,
    this.strutStyle,
    this.textAlign,
    this.textDirection,
    this.locale,
    this.softWrap,
    this.overflow,
    this.textScaler,
    this.maxLines,
    this.semanticsLabel,
    this.semanticsIdentifier,
    this.textWidthBasis,
    this.textHeightBehavior,
    this.selectionColor,
  }) : textSpan = null,
       _variant = _Scale.body;

  /// Creates text at the `caption` step of the type scale.
  const new caption(
    String this.data, {
    super.key,
    this.style,
    this.color,
    this.weight,
    this.decoration,
    this.decorationColor,
    this.decorationStyle,
    this.decorationThickness,
    this.strutStyle,
    this.textAlign,
    this.textDirection,
    this.locale,
    this.softWrap,
    this.overflow,
    this.textScaler,
    this.maxLines,
    this.semanticsLabel,
    this.semanticsIdentifier,
    this.textWidthBasis,
    this.textHeightBehavior,
    this.selectionColor,
  }) : textSpan = null,
       _variant = _Scale.caption;

  /// Creates text at the `micro` step of the type scale.
  const new micro(
    String this.data, {
    super.key,
    this.style,
    this.color,
    this.weight,
    this.decoration,
    this.decorationColor,
    this.decorationStyle,
    this.decorationThickness,
    this.strutStyle,
    this.textAlign,
    this.textDirection,
    this.locale,
    this.softWrap,
    this.overflow,
    this.textScaler,
    this.maxLines,
    this.semanticsLabel,
    this.semanticsIdentifier,
    this.textWidthBasis,
    this.textHeightBehavior,
    this.selectionColor,
  }) : textSpan = null,
       _variant = _Scale.micro;

  /// Creates text at the `nano` step of the type scale.
  const new nano(
    String this.data, {
    super.key,
    this.style,
    this.color,
    this.weight,
    this.decoration,
    this.decorationColor,
    this.decorationStyle,
    this.decorationThickness,
    this.strutStyle,
    this.textAlign,
    this.textDirection,
    this.locale,
    this.softWrap,
    this.overflow,
    this.textScaler,
    this.maxLines,
    this.semanticsLabel,
    this.semanticsIdentifier,
    this.textWidthBasis,
    this.textHeightBehavior,
    this.selectionColor,
  }) : textSpan = null,
       _variant = _Scale.nano;

  /// Creates text at the `button` step of the type scale.
  const new button(
    String this.data, {
    super.key,
    this.style,
    this.color,
    this.weight,
    this.decoration,
    this.decorationColor,
    this.decorationStyle,
    this.decorationThickness,
    this.strutStyle,
    this.textAlign,
    this.textDirection,
    this.locale,
    this.softWrap,
    this.overflow,
    this.textScaler,
    this.maxLines,
    this.semanticsLabel,
    this.semanticsIdentifier,
    this.textWidthBasis,
    this.textHeightBehavior,
    this.selectionColor,
  }) : textSpan = null,
       _variant = _Scale.button;

  /// The text to display.
  ///
  /// Non-null for every constructor except [MText.rich].
  final String? data;

  /// The span to display, for text with multiple styles.
  ///
  /// Non-null for [MText.rich] only.
  final InlineSpan? textSpan;

  /// An explicit style, overriding the ambient or themed base style.
  final TextStyle? style;

  /// The colour of the text, applied over the resolved base style.
  final Color? color;

  /// The glyph weight.
  ///
  /// For a scale variant this selects which of the theme's regular, medium or
  /// bold styles the step renders at (w500, w600 and w700); for plain text it
  /// overrides the resolved style's weight to the same three values. Null
  /// leaves the base style's own weight in place, so a variant renders at the
  /// weight the scale already declares.
  final MFontWeight? weight;

  /// The decoration to paint behind the text.
  final TextDecoration? decoration;

  /// The colour of [decoration].
  final Color? decorationColor;

  /// The style [decoration] is drawn in.
  final TextDecorationStyle? decorationStyle;

  /// The thickness of [decoration].
  final double? decorationThickness;

  /// The strut style to use, controlling the vertical metrics of each line.
  final StrutStyle? strutStyle;

  /// How the text should be aligned horizontally.
  final TextAlign? textAlign;

  /// The text direction, defaulting to the ambient [Directionality].
  final TextDirection? textDirection;

  /// The locale used to select locale-specific glyphs.
  final Locale? locale;

  /// Whether the text should break at soft line breaks.
  final bool? softWrap;

  /// How visual overflow should be handled.
  final TextOverflow? overflow;

  /// The text scaling strategy, defaulting to `MediaQuery.textScalerOf`.
  final TextScaler? textScaler;

  /// The maximum number of lines before the text is truncated by [overflow].
  final int? maxLines;

  /// An alternative semantics label for this text.
  final String? semanticsLabel;

  /// An identifier for this text in the semantics tree.
  final String? semanticsIdentifier;

  /// How the text's intrinsic width is measured.
  final TextWidthBasis? textWidthBasis;

  /// How the text's height is measured.
  final TextHeightBehavior? textHeightBehavior;

  /// The colour of the selection highlight behind the text.
  final Color? selectionColor;

  /// The type-scale step this widget renders at, if any.
  final _Scale? _variant;

  /// Whether any override needs to be applied over the base style.
  bool get _hasOverrides =>
      color != null ||
      decoration != null ||
      decorationColor != null ||
      decorationStyle != null ||
      decorationThickness != null ||
      (_variant == null && weight != null);

  /// The [FontWeight] a plain [MFontWeight] maps to.
  ///
  /// Mirrors the weights [MTextTheme] publishes for a scale step, so a weight
  /// means the same thing whether it selected a themed style or was applied to
  /// plain text. Note that the theme's "regular" is w500, not w400 — it is the
  /// lightest of the three weights the scale defines.
  static FontWeight _fontWeightOf(MFontWeight weight) => switch (weight) {
    MFontWeight.regular => FontWeight.w500,
    MFontWeight.medium => FontWeight.w600,
    MFontWeight.bold => FontWeight.bold,
  };

  /// The regular, medium and bold styles a scale step publishes.
  (TextStyle, TextStyle, TextStyle) _triple(MTextTheme theme) {
    return switch (_variant!) {
      _Scale.heading1 => (
        theme.heading1Regular,
        theme.heading1Medium,
        theme.heading1Bold,
      ),
      _Scale.heading2 => (
        theme.heading2Regular,
        theme.heading2Medium,
        theme.heading2Bold,
      ),
      _Scale.heading3 => (
        theme.heading3Regular,
        theme.heading3Medium,
        theme.heading3Bold,
      ),
      _Scale.subheading => (
        theme.subheadingRegular,
        theme.subheadingMedium,
        theme.subheadingBold,
      ),
      _Scale.body => (theme.bodyRegular, theme.bodyMedium, theme.bodyBold),
      _Scale.caption => (
        theme.captionRegular,
        theme.captionMedium,
        theme.captionBold,
      ),
      _Scale.micro => (theme.microRegular, theme.microMedium, theme.microBold),
      _Scale.nano => (theme.nanoRegular, theme.nanoMedium, theme.nanoBold),
      // The scale publishes a single weight for buttons, so all three
      // selections resolve to it rather than forcing a weight it never defined.
      _Scale.button => (theme.button, theme.button, theme.button),
    };
  }

  /// The base style this widget paints with, before its own overrides.
  ///
  /// Null means "inherit the ambient style", which is [Text]'s behaviour and
  /// the reason plain text is not copied through [TextStyle.copyWith].
  TextStyle? _resolveStyle(BuildContext context) {
    if (_variant != null) {
      final triple = _triple(MTextTheme.of(context));
      final base = switch (weight) {
        MFontWeight.medium => triple.$2,
        MFontWeight.bold => triple.$3,
        _ => triple.$1,
      };
      // The step's weight is already encoded in `base`, so no override is
      // needed for it.
      return base.copyWith(
        color: color,
        decoration: decoration,
        decorationColor: decorationColor,
        decorationStyle: decorationStyle,
        decorationThickness: decorationThickness,
      );
    }

    if (style == null && !_hasOverrides) return null;

    return (style ?? DefaultTextStyle.of(context).style).copyWith(
      color: color,
      decoration: decoration,
      decorationColor: decorationColor,
      decorationStyle: decorationStyle,
      decorationThickness: decorationThickness,
      fontWeight: weight == null ? null : _fontWeightOf(weight!),
    );
  }

  @override
  Widget build(BuildContext context) {
    final resolved = _resolveStyle(context);

    if (textSpan != null) {
      return Text.rich(
        textSpan!,
        key: key,
        style: resolved,
        strutStyle: strutStyle,
        textAlign: textAlign,
        textDirection: textDirection,
        locale: locale,
        softWrap: softWrap,
        overflow: overflow,
        textScaler: textScaler,
        maxLines: maxLines,
        semanticsLabel: semanticsLabel,
        semanticsIdentifier: semanticsIdentifier,
        textWidthBasis: textWidthBasis,
        textHeightBehavior: textHeightBehavior,
        selectionColor: selectionColor,
      );
    }

    return Text(
      data!,
      key: key,
      style: resolved,
      strutStyle: strutStyle,
      textAlign: textAlign,
      textDirection: textDirection,
      locale: locale,
      softWrap: softWrap,
      overflow: overflow,
      textScaler: textScaler,
      maxLines: maxLines,
      semanticsLabel: semanticsLabel,
      semanticsIdentifier: semanticsIdentifier,
      textWidthBasis: textWidthBasis,
      textHeightBehavior: textHeightBehavior,
      selectionColor: selectionColor,
    );
  }
}
