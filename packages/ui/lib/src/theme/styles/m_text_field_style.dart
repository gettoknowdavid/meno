import 'package:flutter/material.dart';
import 'package:meno_ui/src/m_internal.dart';
import 'package:meno_ui/src/styles.dart';
import 'package:meno_ui/src/theme/m_color.dart';
import 'package:meno_ui/src/theme/m_color_scheme.dart';
import 'package:meno_ui/src/theme/styles/m_text_theme.dart';

/// A theme extension for customizing the appearance of text fields in the
/// Meno design system.
///
/// This class defines various styles and colors for different states of a
/// text field, such as text style, error text style, hint text style, label
/// text style, counter text style, icon color, fill color, border styles,
/// and more.
///
/// Example usage:
/// ```dart
/// final textFieldStyle = MTextFieldStyle.of(context);
/// ```
class MTextFieldStyle extends ThemeExtension<MTextFieldStyle> {
  /// Creates an [MTextFieldStyle] instance with the provided styles for text
  /// fields.
  const new({
    required this.textStyle,
    required this.errorTextStyle,
    required this.hintTextStyle,
    required this.labelTextStyle,
    required this.counterTextStyle,
    required this.textColor,
    required this.iconColor,
    required this.fillColor,
    required this.fillColorDisabled,
    required this.counterBgColor,
    required this.counterTextColor,
    required this.counterBgColorDisabled,
    required this.counterTextColorDisabled,
    required this.errorColor,
    required this.border,
    required this.borderFocused,
    required this.borderDisabled,
    required this.borderError,
  });

  /// Creates a default [MTextFieldStyle] based on the given [MColorScheme]
  /// and [MTextTheme].
  ///
  /// This factory constructor initializes the text field styles using the
  /// provided color scheme and text theme, adjusting colors and styles for
  /// light and dark themes.
  ///
  /// - [colors]: The color scheme to use for the text field styles.
  /// - [textTheme]: The text theme to use for the text field styles.
  ///
  /// Returns a new [MTextFieldStyle] instance with the default styles applied.
  factory $default(MColorScheme colors, MTextTheme textTheme) {
    final isLight = colors.brightness == Brightness.light;
    return MTextFieldStyle(
      textStyle: textTheme.captionRegular,
      errorTextStyle: textTheme.captionRegular,
      hintTextStyle: textTheme.captionRegular.copyWith(
        color: colors.onBackgroundVariant,
      ),
      labelTextStyle: textTheme.captionMedium,
      counterTextStyle: textTheme.microMedium,
      iconColor: colors.onBackground,
      fillColor: colors.background,
      fillColorDisabled: colors.disabledContainer,
      counterBgColor: MInternal.resolve(
        isLight,
        MColor.primary50,
        MColor.counter,
      ),
      counterTextColor: MInternal.resolve(
        isLight,
        MColor.primary300,
        MColor.primary60,
      ),
      counterBgColorDisabled: colors.disabledContainer,
      counterTextColorDisabled: colors.disabled,
      textColor: colors.onBackground,
      errorColor: colors.error,
      border: const OutlineInputBorder(
        borderRadius: Corners.md,
        borderSide: BorderSide(color: MColor.grey50),
      ),
      borderFocused: OutlineInputBorder(
        borderRadius: Corners.md,
        borderSide: BorderSide(color: colors.primary, width: 2),
      ),
      borderDisabled: const OutlineInputBorder(
        borderRadius: Corners.md,
        borderSide: BorderSide.none,
      ),
      borderError: OutlineInputBorder(
        borderRadius: Corners.md,
        borderSide: BorderSide(color: colors.error, width: 2),
      ),
    );
  }

  /// The field's [TextStyle]
  final TextStyle textStyle;

  /// The field's [TextStyle] for an error state
  final TextStyle errorTextStyle;

  /// The field's hint [TextStyle]
  final TextStyle hintTextStyle;

  /// The field's label [TextStyle]
  final TextStyle labelTextStyle;

  /// The field's counter [TextStyle]
  final TextStyle counterTextStyle;

  /// The field's text color
  final Color textColor;

  /// The field's icon color
  final Color iconColor;

  /// The field's fill color in the normal state
  final Color fillColor;

  /// The field's fill color in the disabled state
  final Color fillColorDisabled;

  /// The field's counter container background color
  final Color counterBgColor;

  /// The field's counter text color
  final Color counterTextColor;

  /// The field's counter container background color in the disabled state
  final Color counterBgColorDisabled;

  /// The field's counter text color in the disabled state
  final Color counterTextColorDisabled;

  /// The field's error text color in the error state
  final Color errorColor;

  /// The field's border
  final InputBorder border;

  /// The field's focused border
  final InputBorder borderFocused;

  /// The field's disabled border
  final InputBorder borderDisabled;

  /// The field's error border
  final InputBorder borderError;

  /// Retrieves the [MTextFieldStyle] extension from the closest [Theme]
  /// instance that encloses the given [context].
  ///
  /// This method searches for the nearest [Theme] widget in the widget tree
  /// and returns the [MTextFieldStyle] extension if it exists. If no
  /// [MTextFieldStyle] extension is found, this method returns null.
  ///
  /// Example usage:
  /// ```dart
  /// final textFieldStyle = MTextFieldStyle.of(context);
  /// ```
  ///
  /// - [context]: The build context from which to retrieve the
  /// [MTextFieldStyle] extension.
  ///
  /// Returns the [MTextFieldStyle] extension if found, or null if no
  /// [MTextFieldStyle] extension is available in the closest [Theme] instance.
  static MTextFieldStyle of(BuildContext context) =>
      Theme.of(context).extension<MTextFieldStyle>()!;

  @override
  ThemeExtension<MTextFieldStyle> copyWith({
    TextStyle? textStyle,
    TextStyle? errorTextStyle,
    TextStyle? hintTextStyle,
    TextStyle? labelTextStyle,
    TextStyle? counterTextStyle,
    Color? textColor,
    Color? iconColor,
    Color? fillColor,
    Color? fillColorDisabled,
    Color? counterBgColor,
    Color? counterTextColor,
    Color? counterBgColorDisabled,
    Color? counterTextColorDisabled,
    Color? errorColor,
    InputBorder? border,
    InputBorder? borderFocused,
    InputBorder? borderDisabled,
    InputBorder? borderError,
  }) {
    return MTextFieldStyle(
      textStyle: textStyle ?? this.textStyle,
      errorTextStyle: errorTextStyle ?? this.errorTextStyle,
      hintTextStyle: hintTextStyle ?? this.hintTextStyle,
      labelTextStyle: labelTextStyle ?? this.labelTextStyle,
      counterTextStyle: counterTextStyle ?? this.counterTextStyle,
      textColor: textColor ?? this.textColor,
      iconColor: iconColor ?? this.iconColor,
      fillColor: fillColor ?? this.fillColor,
      fillColorDisabled: fillColorDisabled ?? this.fillColorDisabled,
      counterBgColor: counterBgColor ?? this.counterBgColor,
      counterTextColor: counterTextColor ?? this.counterTextColor,
      counterBgColorDisabled:
          counterBgColorDisabled ?? this.counterBgColorDisabled,
      counterTextColorDisabled:
          counterTextColorDisabled ?? this.counterTextColorDisabled,
      errorColor: errorColor ?? this.errorColor,
      border: border ?? this.border,
      borderFocused: borderFocused ?? this.borderFocused,
      borderDisabled: borderDisabled ?? this.borderDisabled,
      borderError: borderError ?? this.borderError,
    );
  }

  @override
  ThemeExtension<MTextFieldStyle> lerp(MTextFieldStyle? other, double t) {
    return MTextFieldStyle(
      textStyle: TextStyle.lerp(textStyle, other?.textStyle, t)!,
      errorTextStyle: TextStyle.lerp(errorTextStyle, other?.errorTextStyle, t)!,
      hintTextStyle: TextStyle.lerp(hintTextStyle, other?.hintTextStyle, t)!,
      labelTextStyle: TextStyle.lerp(labelTextStyle, other?.labelTextStyle, t)!,
      counterTextStyle: TextStyle.lerp(
        counterTextStyle,
        other?.counterTextStyle,
        t,
      )!,
      textColor: Color.lerp(textColor, other?.textColor, t)!,
      iconColor: Color.lerp(iconColor, other?.iconColor, t)!,
      fillColor: Color.lerp(fillColor, other?.fillColor, t)!,
      fillColorDisabled: Color.lerp(
        fillColorDisabled,
        other?.fillColorDisabled,
        t,
      )!,
      counterBgColor: Color.lerp(counterBgColor, other?.counterBgColor, t)!,
      counterTextColor: Color.lerp(
        counterTextColor,
        other?.counterTextColor,
        t,
      )!,
      counterBgColorDisabled: Color.lerp(
        counterBgColorDisabled,
        other?.counterBgColorDisabled,
        t,
      )!,
      counterTextColorDisabled: Color.lerp(
        counterTextColorDisabled,
        other?.counterTextColorDisabled,
        t,
      )!,
      errorColor: Color.lerp(errorColor, other?.errorColor, t)!,
      border: border,
      borderFocused: borderFocused,
      borderDisabled: borderDisabled,
      borderError: borderError,
    );
  }
}
