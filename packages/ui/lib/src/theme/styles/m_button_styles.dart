import 'package:flutter/material.dart';
import 'package:meno_ui/src/m_internal.dart';
import 'package:meno_ui/src/styles.dart';
import 'package:meno_ui/src/theme/m_color.dart';
import 'package:meno_ui/src/theme/m_color_scheme.dart';
import 'package:meno_ui/src/theme/styles/m_text_theme.dart';

ButtonStyle _baseButtonStyle(MTextTheme textTheme) {
  return ButtonStyle(
    iconSize: MInternal.resolveWith(defaultValue: 14),
    elevation: MInternal.resolveWith(defaultValue: 0),
    fixedSize: MInternal.all(const Size.fromHeight(Insets.xxxl)),
    padding: MInternal.all(const EdgeInsets.fromLTRB(16, 8, 16, 8)),
    shadowColor: MInternal.all(MColor.shadow),
    visualDensity: VisualDensity.compact,
    tapTargetSize: MaterialTapTargetSize.shrinkWrap,
    textStyle: MInternal.all(textTheme.captionMedium),
    shape: MInternal.all(
      const RoundedRectangleBorder(borderRadius: Corners.md),
    ),
  );
}

/// A theme extension for customizing button styles in the
/// Meno design system.
///
/// This class defines various styles for buttons, including
/// primary, secondary, text, success, and danger button styles.
///
/// Example usage:
/// ```dart
/// final buttonStyles = MButtonStyles.of(context);
/// ```
class MButtonStyles extends ThemeExtension<MButtonStyles> {
  /// A theme extension for customizing button styles in the
  /// Meno design system.
  ///
  /// This class defines various styles for buttons, including
  /// primary, secondary, text, success, and danger button styles.
  ///
  /// Example usage:
  /// ```dart
  /// final buttonStyles = MButtonStyles.of(context);
  /// ```
  const new({
    required this.primary,
    required this.secondary,
    required this.text,
    required this.success,
    required this.danger,
  });

  /// Creates a default [MButtonStyles] based on the given [MColorScheme]
  /// and [MTextTheme].
  ///
  /// This factory constructor initializes the button styles using the
  /// provided color scheme and text theme.
  ///
  /// - [colors]: The color scheme to use for the button styles.
  /// - [textTheme]: The text theme to use for the button styles.
  ///
  /// Returns a new [MButtonStyles] instance with the default styles applied.
  factory $default(MColorScheme colors, MTextTheme textTheme) {
    final baseStyle = _baseButtonStyle(textTheme);
    return MButtonStyles(
      primary: baseStyle.copyWith(
        backgroundColor: MInternal.resolveWith(
          defaultValue: colors.primary,
          pressedValue: colors.inversePrimary,
          disabledValue: colors.disabled,
        ),
        foregroundColor: MInternal.resolveWith(
          defaultValue: colors.onPrimary,
          pressedValue: colors.onInversePrimary,
          disabledValue: colors.onDisabled,
        ),
        iconColor: MInternal.resolveWith(
          defaultValue: colors.onPrimary,
          pressedValue: colors.onInversePrimary,
          disabledValue: colors.onDisabled,
        ),
        textStyle: MInternal.all(textTheme.bodyMedium),
      ),
      secondary: baseStyle.copyWith(
        backgroundColor: MInternal.resolveWith(
          defaultValue: MColor.transparent,
          pressedValue: colors.inversePrimary,
          disabledValue: colors.disabled,
        ),
        foregroundColor: MInternal.resolveWith(
          defaultValue: colors.primary,
          pressedValue: colors.onInversePrimary,
          disabledValue: colors.onDisabled,
        ),
        iconColor: MInternal.resolveWith(
          defaultValue: colors.primary,
          pressedValue: colors.onInversePrimary,
          disabledValue: colors.onDisabled,
        ),
        textStyle: MInternal.all(textTheme.bodyMedium),
        side: MInternal.resolveWith(
          defaultValue: BorderSide(color: colors.primary, width: 1.50),
          pressedValue: BorderSide(color: colors.primary, width: 1.50),
          disabledValue: const BorderSide(width: 0),
        ),
      ),
      text: baseStyle.copyWith(
        backgroundColor: MInternal.resolveWith(
          defaultValue: MColor.transparent,
          pressedValue: MColor.transparent,
          disabledValue: colors.disabled,
        ),
        foregroundColor: MInternal.resolveWith(
          defaultValue: colors.primary,
          pressedValue: colors.onInversePrimary,
          disabledValue: colors.onDisabled,
        ),
        iconColor: MInternal.resolveWith(
          defaultValue: colors.primary,
          pressedValue: colors.onInversePrimary,
          disabledValue: colors.onDisabled,
        ),
        visualDensity: VisualDensity.compact,
        tapTargetSize: MaterialTapTargetSize.shrinkWrap,
      ),
      success: baseStyle.copyWith(
        backgroundColor: MInternal.resolveWith(
          defaultValue: colors.success,
          pressedValue: colors.successContainer,
          disabledValue: colors.disabled,
        ),
        foregroundColor: MInternal.resolveWith(
          defaultValue: colors.onSuccess,
          pressedValue: colors.onSuccessContainer,
          disabledValue: colors.onDisabled,
        ),
        iconColor: MInternal.resolveWith(
          defaultValue: colors.onSuccess,
          pressedValue: colors.onSuccessContainer,
          disabledValue: colors.onDisabled,
        ),
      ),
      danger: baseStyle.copyWith(
        backgroundColor: MInternal.resolveWith(
          defaultValue: colors.error,
          pressedValue: colors.errorContainer,
          disabledValue: colors.disabled,
        ),
        foregroundColor: MInternal.resolveWith(
          defaultValue: colors.onError,
          pressedValue: colors.onErrorContainer,
          disabledValue: colors.onDisabled,
        ),
        iconColor: MInternal.resolveWith(
          defaultValue: colors.onError,
          pressedValue: colors.onErrorContainer,
          disabledValue: colors.onDisabled,
        ),
        shape: MInternal.all(
          const RoundedRectangleBorder(borderRadius: Corners.sm),
        ),
      ),
    );
  }

  /// The style for primary buttons.
  final ButtonStyle primary;

  /// The style for secondary buttons.
  final ButtonStyle secondary;

  /// The style for text buttons.
  final ButtonStyle text;

  /// The style for success buttons.
  final ButtonStyle success;

  /// The style for danger buttons.
  final ButtonStyle danger;

  /// Retrieves the [MButtonStyles] extension from the closest [Theme] instance
  /// that encloses the given [context].
  ///
  /// This method searches for the nearest [Theme] widget in the widget tree
  /// and returns the [MButtonStyles] extension if it exists. If no
  /// [MButtonStyles] extension is found, this method returns null.
  ///
  /// Example usage:
  /// ```dart
  /// final buttonStyles = MButtonStyles.of(context);
  /// ```
  ///
  /// - [context]: The build context from which to retrieve the [MButtonStyles]
  /// extension.
  ///
  /// Returns the [MButtonStyles] extension if found, or null if no
  /// [MButtonStyles] extension is available in the closest [Theme] instance.
  static MButtonStyles? of(BuildContext context) {
    return Theme.of(context).extension<MButtonStyles>();
  }

  /// Provides the [ElevatedButtonThemeData] based on the primary button style.
  ///
  /// This getter constructs an [ElevatedButtonThemeData] using the primary
  /// button style defined in the current instance of [MButtonStyles].
  ///
  /// Returns an [ElevatedButtonThemeData] instance with the primary style
  /// applied.
  ElevatedButtonThemeData get elevatedButtonTheme {
    return ElevatedButtonThemeData(style: primary);
  }

  /// Provides the [OutlinedButtonThemeData] based on the secondary button
  /// style.
  ///
  /// This getter constructs an [OutlinedButtonThemeData] using the secondary
  /// button style defined in the current instance of [MButtonStyles].
  ///
  /// Returns an [OutlinedButtonThemeData] instance with the secondary style
  /// applied.
  OutlinedButtonThemeData get outlinedButtonTheme {
    return OutlinedButtonThemeData(style: secondary);
  }

  /// Provides the [TextButtonThemeData] based on the text button style.
  ///
  /// This getter constructs a [TextButtonThemeData] using the text button style
  /// defined in the current instance of [MButtonStyles].
  ///
  /// Returns a [TextButtonThemeData] instance with the text style applied.
  TextButtonThemeData get textButtonTheme {
    return TextButtonThemeData(style: text);
  }

  @override
  ThemeExtension<MButtonStyles> copyWith({
    ButtonStyle? primary,
    ButtonStyle? secondary,
    ButtonStyle? text,
    ButtonStyle? success,
    ButtonStyle? danger,
  }) {
    return MButtonStyles(
      primary: primary ?? this.primary,
      secondary: secondary ?? this.secondary,
      text: text ?? this.text,
      success: success ?? this.success,
      danger: danger ?? this.danger,
    );
  }

  @override
  ThemeExtension<MButtonStyles> lerp(
    ThemeExtension<MButtonStyles>? other,
    double t,
  ) {
    if (other is! MButtonStyles) return this;
    return MButtonStyles(
      primary: ButtonStyle.lerp(primary, other.primary, t)!,
      secondary: ButtonStyle.lerp(secondary, other.secondary, t)!,
      text: ButtonStyle.lerp(text, other.text, t)!,
      success: ButtonStyle.lerp(success, other.success, t)!,
      danger: ButtonStyle.lerp(danger, other.danger, t)!,
    );
  }
}
