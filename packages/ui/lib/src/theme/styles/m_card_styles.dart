import 'package:flutter/material.dart';
import 'package:meno_ui/src/m_internal.dart';
import 'package:meno_ui/src/styles.dart';
import 'package:meno_ui/src/theme/m_color.dart';
import 'package:meno_ui/src/theme/m_color_scheme.dart';
import 'package:meno_ui/src/theme/styles/m_text_theme.dart';

/// A theme extension for customizing card styles in the
/// Meno design system.
///
/// This class defines various styles for cards, including
/// background colors, text styles, and padding.
///
/// Example usage:
/// ```dart
/// final cardStyles = MCardStyles.of(context);
/// ```

class MCardStyles extends ThemeExtension<MCardStyles> {
  /// Creates a new instance of [MCardStyles].
  ///
  /// The constructor allows you to specify custom styles for cards.
  ///
  /// - [backgroundColor]: The background color of the card.
  /// - [titleColor]: The color of the card title.
  /// - [hostColor]: The color of the host text in the card.
  /// - [titleStyle]: The text style for the card title.
  /// - [hostStyle]: The text style for the host text in the card.
  /// - [nSubtitleColor]: The color of the subtitle text in the card.
  /// - [nBackgroundColor]: The background color for a nested card.
  /// - [nCardContentPadding]: The padding for the content inside a nested card.
  /// - [nBorderRadius]: The border radius for a nested card.
  /// - [nTitleTextStyle]: The text style for the title in a nested card.
  /// - [nSubtitleTextStyle]: The text style for the subtitle in a nested card.
  const new({
    required this.backgroundColor,
    required this.titleColor,
    required this.hostColor,
    required this.titleStyle,
    required this.hostStyle,
    required this.nSubtitleColor,
    required this.nBackgroundColor,
    required this.nCardContentPadding,
    required this.nBorderRadius,
    required this.nTitleTextStyle,
    required this.nSubtitleTextStyle,
  });

  /// Creates a default [MCardStyles] based on the given [MColorScheme]
  /// and [MTextTheme].
  ///
  /// This factory constructor initializes the card styles using the
  /// provided color scheme and text theme.
  ///
  /// - [colors]: The color scheme to use for the card styles.
  /// - [textTheme]: The text theme to use for the card styles.
  ///
  /// Returns a new [MCardStyles] instance with the default styles applied.
  factory $default(MColorScheme colors, MTextTheme textTheme) {
    final isLight = colors.brightness == Brightness.light;
    return MCardStyles(
      backgroundColor: MInternal.resolve(
        isLight,
        MColor.white,
        MColor.primaryAlt,
      ),
      titleColor: colors.onBackground,
      hostColor: MInternal.resolve(isLight, MColor.grey80, MColor.grey30),
      titleStyle: textTheme.captionMedium,
      hostStyle: textTheme.captionRegular,
      nSubtitleColor: colors.onBackgroundVariant,
      nBackgroundColor: colors.surfaceTint,
      nCardContentPadding: const EdgeInsets.symmetric(
        horizontal: Insets.sm,
        vertical: Insets.lg,
      ),
      nBorderRadius: Corners.lg,
      nTitleTextStyle: textTheme.captionRegular,
      nSubtitleTextStyle: textTheme.microRegular,
    );
  }

  /// The background color of the card.
  final Color backgroundColor;

  /// The color of the card title.
  final Color titleColor;

  /// The color of the host text in the card.
  final Color hostColor;

  /// The text style for the card title.
  final TextStyle titleStyle;

  /// The text style for the host text in the card.
  final TextStyle hostStyle;

  /// The color of the subtitle text in the card.
  final Color nSubtitleColor;

  /// The background color for a nested card.
  final Color nBackgroundColor;

  /// The padding for the content inside a nested card.
  final EdgeInsetsGeometry nCardContentPadding;

  /// The border radius for a nested card.
  final BorderRadiusGeometry nBorderRadius;

  /// The text style for the title in a nested card.
  final TextStyle nTitleTextStyle;

  /// The text style for the subtitle in a nested card.
  final TextStyle nSubtitleTextStyle;

  /// Provides the [CardTheme] based on the current card styles.
  ///
  /// This getter constructs a [CardTheme] using the properties
  /// defined in the current instance of [MCardStyles].
  ///
  /// Returns a [CardTheme] instance with the styles applied.
  CardThemeData get cardTheme => CardThemeData(color: backgroundColor);

  /// Retrieves the [MCardStyles] extension from the closest [Theme] instance
  /// that encloses the given [context].
  ///
  /// This method searches for the nearest [Theme] widget in the widget tree
  /// and returns the [MCardStyles] extension if it exists. If no [MCardStyles]
  /// extension is found, this method returns null.
  ///
  /// Example usage:
  /// ```dart
  /// final cardStyles = MCardStyles.of(context);
  /// ```
  ///
  /// - [context]: The build context from which to retrieve the [MCardStyles]
  /// extension.
  ///
  /// Returns the [MCardStyles] extension if found, or null if no [MCardStyles]
  /// extension is available in the closest [Theme] instance.
  static MCardStyles of(BuildContext context) {
    return Theme.of(context).extension<MCardStyles>()!;
  }

  @override
  ThemeExtension<MCardStyles> copyWith({
    Color? backgroundColor,
    Color? titleColor,
    Color? hostColor,
    TextStyle? titleStyle,
    TextStyle? hostStyle,
    Color? nSubtitleColor,
    Color? nBackgroundColor,
    EdgeInsetsGeometry? nCardContentPadding,
    BorderRadiusGeometry? nBorderRadius,
    TextStyle? nTitleTextStyle,
    TextStyle? nSubtitleTextStyle,
  }) {
    return MCardStyles(
      backgroundColor: backgroundColor ?? this.backgroundColor,
      titleColor: titleColor ?? this.titleColor,
      hostColor: hostColor ?? this.hostColor,
      titleStyle: titleStyle ?? this.titleStyle,
      hostStyle: hostStyle ?? this.hostStyle,
      nSubtitleColor: nSubtitleColor ?? this.nSubtitleColor,
      nBackgroundColor: nBackgroundColor ?? this.nBackgroundColor,
      nCardContentPadding: nCardContentPadding ?? this.nCardContentPadding,
      nBorderRadius: nBorderRadius ?? this.nBorderRadius,
      nTitleTextStyle: nTitleTextStyle ?? this.nTitleTextStyle,
      nSubtitleTextStyle: nSubtitleTextStyle ?? this.nSubtitleTextStyle,
    );
  }

  @override
  ThemeExtension<MCardStyles> lerp(
    covariant ThemeExtension<MCardStyles>? other,
    double t,
  ) {
    if (other is! MCardStyles) return this;
    return MCardStyles(
      backgroundColor: Color.lerp(backgroundColor, other.backgroundColor, t)!,
      titleColor: Color.lerp(titleColor, other.titleColor, t)!,
      hostColor: Color.lerp(hostColor, other.hostColor, t)!,
      titleStyle: TextStyle.lerp(titleStyle, other.titleStyle, t)!,
      hostStyle: TextStyle.lerp(hostStyle, other.hostStyle, t)!,
      nSubtitleColor: Color.lerp(nSubtitleColor, other.nSubtitleColor, t)!,
      nBackgroundColor: Color.lerp(
        nBackgroundColor,
        other.nBackgroundColor,
        t,
      )!,
      nCardContentPadding: EdgeInsetsGeometry.lerp(
        nCardContentPadding,
        other.nCardContentPadding,
        t,
      )!,
      nBorderRadius: BorderRadiusGeometry.lerp(
        nBorderRadius,
        other.nBorderRadius,
        t,
      )!,
      nTitleTextStyle: TextStyle.lerp(
        nTitleTextStyle,
        other.nTitleTextStyle,
        t,
      )!,
      nSubtitleTextStyle: TextStyle.lerp(
        nSubtitleTextStyle,
        other.nSubtitleTextStyle,
        t,
      )!,
    );
  }
}
