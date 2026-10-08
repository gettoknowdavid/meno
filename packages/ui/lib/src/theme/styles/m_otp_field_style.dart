import 'package:flutter/material.dart';
import 'package:meno_ui/src/theme/m_color.dart';
import 'package:meno_ui/src/theme/m_color_scheme.dart';
import 'package:meno_ui/src/theme/styles/m_text_theme.dart';

/// A theme extension for customizing the appearance of OTP fields in the
/// Meno design system.
///
/// This class defines various styles and colors for different states of an
/// OTP field, such as text style, fill color, border styles, and more.
///
/// Example usage:
/// ```dart
/// final otpFieldStyles = MOtpFieldStyles.of(context);
/// ```
class MOtpFieldStyles extends ThemeExtension<MOtpFieldStyles> {
  /// Creates a new instance of [MOtpFieldStyles].
  ///
  /// The constructor allows you to specify custom styles and colors for the
  /// OTP field.
  ///
  /// - [textStyle]: The text style for the OTP field.
  /// - [textColor]: The color of the text in the OTP field.
  /// - [fillColor]: The fill color of the OTP field.
  /// - [fillColorDisabled]: The fill color of the OTP field when disabled.
  /// - [errorColor]: The color of the OTP field when there is an error.
  /// - [border]: The border of the OTP field.
  /// - [borderFocused]: The border of the OTP field when focused.
  /// - [borderError]: The border of the OTP field when there is an error.
  const new({
    required this.textStyle,
    required this.textColor,
    required this.fillColor,
    required this.fillColorDisabled,
    required this.errorColor,
    required this.border,
    required this.borderFocused,
    required this.borderError,
  });

  /// Creates a default [MOtpFieldStyles] based on the given [MColorScheme]
  /// and [MTextTheme].
  ///
  /// This factory constructor initializes the OTP field styles using the
  /// provided color scheme and text theme.
  ///
  /// - [colors]: The color scheme to use for the OTP field styles.
  /// - [textTheme]: The text theme to use for the OTP field styles.
  ///
  /// Returns a new [MOtpFieldStyles] instance with the default styles applied.
  factory $default(MColorScheme colors, MTextTheme textTheme) {
    return MOtpFieldStyles(
      textStyle: textTheme.captionRegular,
      fillColor: colors.background,
      fillColorDisabled: colors.disabledContainer,
      textColor: colors.onBackground,
      errorColor: colors.error,
      border: Border.all(color: MColor.grey50),
      borderFocused: Border.all(color: colors.outline, width: 2),
      borderError: Border.all(color: colors.error, width: 2),
    );
  }

  /// The text style for the OTP field.
  final TextStyle textStyle;

  /// The color of the text in the OTP field.
  final Color textColor;

  /// The fill color of the OTP field.
  final Color fillColor;

  /// The fill color of the OTP field when disabled.
  final Color fillColorDisabled;

  /// The color of the OTP field when there is an error.
  final Color errorColor;

  /// The border of the OTP field.
  final BoxBorder border;

  /// The border of the OTP field when focused.
  final BoxBorder borderFocused;

  /// The border of the OTP field when there is an error.
  final BoxBorder borderError;

  /// Retrieves the [MOtpFieldStyles] extension from the closest [Theme]
  /// instance that encloses the given [context].
  ///
  /// This method searches for the nearest [Theme] widget in the widget tree
  /// and returns the [MOtpFieldStyles] extension if it exists. If no
  /// [MOtpFieldStyles] extension is found, this method returns null.
  ///
  /// Example usage:
  /// ```dart
  /// final otpFieldStyles = MOtpFieldStyles.of(context);
  /// ```
  ///
  /// - [context]: The build context from which to retrieve the
  /// [MOtpFieldStyles] extension.
  ///
  /// Returns the [MOtpFieldStyles] extension if found, or null if no
  /// [MOtpFieldStyles] extension is available in the closest [Theme] instance.
  static MOtpFieldStyles of(BuildContext context) {
    return Theme.of(context).extension<MOtpFieldStyles>()!;
  }

  @override
  ThemeExtension<MOtpFieldStyles> copyWith({
    TextStyle? textStyle,
    Color? textColor,
    Color? fillColor,
    Color? fillColorDisabled,
    Color? errorColor,
    BoxBorder? border,
    BoxBorder? borderFocused,
    BoxBorder? borderError,
  }) {
    return MOtpFieldStyles(
      textStyle: textStyle ?? this.textStyle,
      textColor: textColor ?? this.textColor,
      fillColor: fillColor ?? this.fillColor,
      fillColorDisabled: fillColorDisabled ?? this.fillColorDisabled,
      errorColor: errorColor ?? this.errorColor,
      border: border ?? this.border,
      borderFocused: borderFocused ?? this.borderFocused,
      borderError: borderError ?? this.borderError,
    );
  }

  @override
  ThemeExtension<MOtpFieldStyles> lerp(MOtpFieldStyles? other, double t) {
    return MOtpFieldStyles(
      textStyle: TextStyle.lerp(textStyle, other?.textStyle, t)!,
      textColor: Color.lerp(textColor, other?.textColor, t)!,
      fillColor: Color.lerp(fillColor, other?.fillColor, t)!,
      fillColorDisabled: Color.lerp(
        fillColorDisabled,
        other?.fillColorDisabled,
        t,
      )!,
      errorColor: Color.lerp(errorColor, other?.errorColor, t)!,
      border: BoxBorder.lerp(border, other?.border, t)!,
      borderFocused: BoxBorder.lerp(borderFocused, other?.borderFocused, t)!,
      borderError: BoxBorder.lerp(borderError, other?.borderError, t)!,
    );
  }
}
