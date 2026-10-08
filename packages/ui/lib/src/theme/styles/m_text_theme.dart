import 'package:flutter/material.dart';
import 'package:meno_ui/src/gen/fonts.gen.dart';
import 'package:meno_ui/src/theme/m_color_scheme.dart';

/// A custom theme extension for managing text styles within the app.
///
/// The [MTextTheme] class extends [ThemeExtension] to define a collection of
/// text styles used throughout the application. This helps in maintaining
/// consistent typography and allows for easy customization of text styles
/// based on the application's color scheme.
class MTextTheme extends ThemeExtension<MTextTheme> {
  /// Creates an [MTextTheme] instance with the provided text styles.
  ///
  /// All text styles are optional and can be customized individually. If a
  /// text style is not provided, it defaults to null.
  const new({
    required this.heading1Regular,
    required this.heading1Bold,
    required this.heading1Medium,
    required this.heading2Regular,
    required this.heading2Bold,
    required this.heading2Medium,
    required this.heading3Regular,
    required this.heading3Bold,
    required this.heading3Medium,
    required this.subheadingRegular,
    required this.subheadingBold,
    required this.subheadingMedium,
    required this.bodyRegular,
    required this.bodyBold,
    required this.bodyMedium,
    required this.captionRegular,
    required this.captionBold,
    required this.captionMedium,
    required this.microRegular,
    required this.microBold,
    required this.microMedium,
    required this.nanoRegular,
    required this.nanoBold,
    required this.nanoMedium,
    required this.button,
    required this.countDown,
  });

  /// Provides the default [MTextTheme] for the app based on the given [colors].
  ///
  /// This factory method initializes an [MTextTheme] instance using
  /// a predefined set of text styles, which are derived from the
  /// provided [MColorScheme].
  factory $default(MColorScheme colors) {
    final typography = _Typography(colors);
    return MTextTheme(
      heading1Regular: typography.heading1Regular,
      heading1Bold: typography.heading1Bold,
      heading1Medium: typography.heading1Medium,
      heading2Regular: typography.heading2Regular,
      heading2Bold: typography.heading2Bold,
      heading2Medium: typography.heading2Medium,
      heading3Regular: typography.heading3Regular,
      heading3Bold: typography.heading3Bold,
      heading3Medium: typography.heading3Medium,
      subheadingRegular: typography.subheadingRegular,
      subheadingBold: typography.subheadingBold,
      subheadingMedium: typography.subheadingMedium,
      bodyRegular: typography.bodyRegular,
      bodyBold: typography.bodyBold,
      bodyMedium: typography.bodyMedium,
      captionRegular: typography.captionRegular,
      captionBold: typography.captionBold,
      captionMedium: typography.captionMedium,
      microRegular: typography.microRegular,
      microBold: typography.microBold,
      microMedium: typography.microMedium,
      nanoRegular: typography.nanoRegular,
      nanoBold: typography.nanoBold,
      nanoMedium: typography.nanoMedium,
      button: typography.button,
      countDown: typography.countDown,
    );
  }

  /// Heading 1 Regular
  final TextStyle heading1Regular;

  /// Heading 1 Medium
  final TextStyle heading1Medium;

  /// Heading 1 Bold
  final TextStyle heading1Bold;

  /// Heading 2 Regular
  final TextStyle heading2Regular;

  /// Heading 2 Medium
  final TextStyle heading2Medium;

  /// Heading 2 Bold
  final TextStyle heading2Bold;

  /// Heading 3 Regular
  final TextStyle heading3Regular;

  /// Heading 3 Medium
  final TextStyle heading3Medium;

  /// Heading 3 Bold
  final TextStyle heading3Bold;

  /// Subheading Regular
  final TextStyle subheadingRegular;

  /// Subheading Medium
  final TextStyle subheadingMedium;

  /// Subheading Bold
  final TextStyle subheadingBold;

  /// Body Regular
  final TextStyle bodyRegular;

  /// Body Medium
  final TextStyle bodyMedium;

  /// Body Bold
  final TextStyle bodyBold;

  /// Caption Regular
  final TextStyle captionRegular;

  /// Caption Medium
  final TextStyle captionMedium;

  /// Caption Bold
  final TextStyle captionBold;

  /// Micro Regular
  final TextStyle microRegular;

  /// Micro Medium
  final TextStyle microMedium;

  /// Micro Bold
  final TextStyle microBold;

  /// Nano Regular
  final TextStyle nanoRegular;

  /// Nano Medium
  final TextStyle nanoMedium;

  /// Nano Bold
  final TextStyle nanoBold;

  /// Button Medium
  final TextStyle button;

  /// Count down number text style
  final TextStyle countDown;

  /// Provides the Global Font Family
  static const String fontFamily = FontFamily.sFProDisplay;

  @override
  ThemeExtension<MTextTheme> copyWith({
    TextStyle? heading1Regular,
    TextStyle? heading1Bold,
    TextStyle? heading1Medium,
    TextStyle? heading2Regular,
    TextStyle? heading2Bold,
    TextStyle? heading2Medium,
    TextStyle? heading3Regular,
    TextStyle? heading3Bold,
    TextStyle? heading3Medium,
    TextStyle? subheadingRegular,
    TextStyle? subheadingBold,
    TextStyle? subheadingMedium,
    TextStyle? bodyRegular,
    TextStyle? bodyBold,
    TextStyle? bodyMedium,
    TextStyle? captionRegular,
    TextStyle? captionBold,
    TextStyle? captionMedium,
    TextStyle? microRegular,
    TextStyle? microBold,
    TextStyle? microMedium,
    TextStyle? nanoRegular,
    TextStyle? nanoBold,
    TextStyle? nanoMedium,
    TextStyle? button,
    TextStyle? countDown,
  }) {
    return MTextTheme(
      heading1Regular: heading1Regular ?? this.heading1Regular,
      heading1Bold: heading1Bold ?? this.heading1Bold,
      heading1Medium: heading1Medium ?? this.heading1Medium,
      heading2Regular: heading2Regular ?? this.heading2Regular,
      heading2Bold: heading2Bold ?? this.heading2Bold,
      heading2Medium: heading2Medium ?? this.heading2Medium,
      heading3Regular: heading3Regular ?? this.heading3Regular,
      heading3Bold: heading3Bold ?? this.heading3Bold,
      heading3Medium: heading3Medium ?? this.heading3Medium,
      subheadingRegular: subheadingRegular ?? this.subheadingRegular,
      subheadingBold: subheadingBold ?? this.subheadingBold,
      subheadingMedium: subheadingMedium ?? this.subheadingMedium,
      bodyRegular: bodyRegular ?? this.bodyRegular,
      bodyBold: bodyBold ?? this.bodyBold,
      bodyMedium: bodyMedium ?? this.bodyMedium,
      captionRegular: captionRegular ?? this.captionRegular,
      captionBold: captionBold ?? this.captionBold,
      captionMedium: captionMedium ?? this.captionMedium,
      microRegular: microRegular ?? this.microRegular,
      microBold: microBold ?? this.microBold,
      microMedium: microMedium ?? this.microMedium,
      nanoRegular: nanoRegular ?? this.nanoRegular,
      nanoBold: nanoBold ?? this.nanoBold,
      nanoMedium: nanoMedium ?? this.nanoMedium,
      button: button ?? this.button,
      countDown: countDown ?? this.countDown,
    );
  }

  @override
  ThemeExtension<MTextTheme> lerp(
    covariant ThemeExtension<MTextTheme>? other,
    double t,
  ) {
    if (other is! MTextTheme) return this;
    return MTextTheme(
      heading1Regular: TextStyle.lerp(
        heading1Regular,
        other.heading1Regular,
        t,
      )!,
      heading1Medium: TextStyle.lerp(heading1Medium, other.heading1Medium, t)!,
      heading1Bold: TextStyle.lerp(heading1Bold, other.heading1Bold, t)!,
      heading2Regular: TextStyle.lerp(
        heading2Regular,
        other.heading2Regular,
        t,
      )!,
      heading2Medium: TextStyle.lerp(heading2Medium, other.heading2Medium, t)!,
      heading2Bold: TextStyle.lerp(heading2Bold, other.heading2Bold, t)!,
      heading3Regular: TextStyle.lerp(
        heading3Regular,
        other.heading3Regular,
        t,
      )!,
      heading3Medium: TextStyle.lerp(heading3Medium, other.heading3Medium, t)!,
      heading3Bold: TextStyle.lerp(heading3Bold, other.heading3Bold, t)!,
      subheadingRegular: TextStyle.lerp(
        subheadingRegular,
        other.subheadingRegular,
        t,
      )!,
      subheadingMedium: TextStyle.lerp(
        subheadingMedium,
        other.subheadingMedium,
        t,
      )!,
      subheadingBold: TextStyle.lerp(subheadingBold, other.subheadingBold, t)!,
      bodyRegular: TextStyle.lerp(bodyRegular, other.bodyRegular, t)!,
      bodyMedium: TextStyle.lerp(bodyMedium, other.bodyMedium, t)!,
      bodyBold: TextStyle.lerp(bodyBold, other.bodyBold, t)!,
      captionRegular: TextStyle.lerp(captionRegular, other.captionRegular, t)!,
      captionMedium: TextStyle.lerp(captionMedium, other.captionMedium, t)!,
      captionBold: TextStyle.lerp(captionBold, other.captionBold, t)!,
      microRegular: TextStyle.lerp(microRegular, other.microRegular, t)!,
      microMedium: TextStyle.lerp(microMedium, other.microMedium, t)!,
      microBold: TextStyle.lerp(microBold, other.microBold, t)!,
      nanoRegular: TextStyle.lerp(nanoRegular, other.nanoRegular, t)!,
      nanoMedium: TextStyle.lerp(nanoMedium, other.nanoMedium, t)!,
      nanoBold: TextStyle.lerp(nanoBold, other.nanoBold, t)!,
      button: TextStyle.lerp(button, other.button, t)!,
      countDown: TextStyle.lerp(countDown, other.countDown, t)!,
    );
  }

  /// Retrieves the [MTextTheme] extension from the closest [Theme] instance
  /// that encloses the given [context].
  ///
  /// This method searches for the nearest [Theme] widget in the widget tree
  /// and returns the [MTextTheme] extension if it exists. If no [MTextTheme]
  /// extension is found, this method returns null.
  ///
  /// The [MTextTheme] extension must be added to the [ThemeData.extensions]
  /// in your theme configuration to be accessible using this method.
  ///
  /// Example usage:
  /// ```dart
  /// final mTextTheme = MTextTheme.of(context);
  /// ```
  ///
  /// - [context]: The build context from which to retrieve the [MTextTheme]
  /// extension.
  ///
  /// Returns the [MTextTheme] extension if found, or null if no [MTextTheme]
  /// extension is available in the closest [Theme] instance.
  static MTextTheme of(BuildContext context) {
    return Theme.of(context).extension<MTextTheme>()!;
  }
}

@immutable
class _Typography {
  new(this._colors);

  final MColorScheme _colors;

  late final TextStyle heading1Regular = _font(32, height: 40);

  late final TextStyle heading1Medium = heading1Regular.copyWith(
    fontWeight: FontWeight.w600,
  );

  late final TextStyle heading1Bold = heading1Regular.copyWith(
    fontWeight: FontWeight.w700,
  );

  late final TextStyle heading2Regular = _font(24, height: 32);

  late final TextStyle heading2Medium = heading2Regular.copyWith(
    fontWeight: FontWeight.w600,
  );

  late final TextStyle heading2Bold = heading2Regular.copyWith(
    fontWeight: FontWeight.w700,
  );

  late final TextStyle heading3Regular = _font(20, height: 24);

  late final TextStyle heading3Medium = heading3Regular.copyWith(
    fontWeight: FontWeight.w600,
  );

  late final TextStyle heading3Bold = heading3Regular.copyWith(
    fontWeight: FontWeight.w700,
  );

  late final TextStyle subheadingRegular = _font(16, height: 24);

  late final TextStyle subheadingMedium = subheadingRegular.copyWith(
    fontWeight: FontWeight.w600,
  );

  late final TextStyle subheadingBold = subheadingRegular.copyWith(
    fontWeight: FontWeight.w700,
  );

  late final TextStyle bodyRegular = _font(16, height: 24);

  late final TextStyle bodyMedium = bodyRegular.copyWith(
    fontWeight: FontWeight.w600,
  );

  late final TextStyle bodyBold = bodyRegular.copyWith(
    fontWeight: FontWeight.w700,
  );

  late final TextStyle captionRegular = _font(14, height: 18);

  late final TextStyle captionMedium = captionRegular.copyWith(
    fontWeight: FontWeight.w600,
  );

  late final TextStyle captionBold = captionRegular.copyWith(
    fontWeight: FontWeight.w700,
  );

  late final TextStyle microRegular = _font(12, height: 16);

  late final TextStyle microMedium = microRegular.copyWith(
    fontWeight: FontWeight.w600,
  );

  late final TextStyle microBold = microRegular.copyWith(
    fontWeight: FontWeight.w700,
  );

  late final TextStyle nanoRegular = _font(10, height: 14);

  late final TextStyle nanoMedium = nanoRegular.copyWith(
    fontWeight: FontWeight.w600,
  );

  late final TextStyle nanoBold = nanoRegular.copyWith(
    fontWeight: FontWeight.w700,
  );

  late final TextStyle button = _font(8, height: 16, weight: FontWeight.w600);

  late final TextStyle countDown = _font(72, weight: FontWeight.w700);

  TextStyle get _style => const TextStyle(fontFamily: FontFamily.sFProDisplay);

  TextStyle _font(
    double size, {
    double? height,
    FontWeight? weight,
    TextDecoration? decoration,
    Color? color,
  }) {
    return _style.copyWith(
      fontSize: size,
      fontFamily: FontFamily.sFProDisplay,
      height: height != null ? (height / size) : _style.height,
      fontWeight: weight ?? FontWeight.w500,
      decoration: decoration,
      color: color ?? _colors.onBackground,
    );
  }
}
