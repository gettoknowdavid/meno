import 'package:flutter/material.dart';
import 'package:meno_ui/src/styles.dart';
import 'package:meno_ui/src/theme/m_color.dart';
import 'package:meno_ui/src/theme/m_color_scheme.dart';
import 'package:meno_ui/src/theme/styles/m_text_theme.dart';

/// A theme extension for customizing global elements in the
/// Meno design system.
///
/// This class defines various styles for global elements such as
/// dividers, SnackBars, and checkboxes.
///
/// Example usage:
/// ```dart
/// final globalStyles = MGlobalStyles.of(context);
/// ```

class MGlobalStyles extends ThemeExtension<MGlobalStyles> {
  /// Creates a new instance of [MGlobalStyles].
  ///
  /// The constructor allows you to specify custom styles for global elements.
  ///
  /// - [dividerColor]: The color of the dividers.
  /// - [snackBarTheme]: The theme for SnackBars.
  /// - [checkboxTheme]: The theme for checkboxes.
  const new({
    required this.dividerColor,
    required this.snackBarTheme,
    required this.checkboxTheme,
  });

  /// Creates a default [MGlobalStyles] based on the given [MColorScheme]
  /// and [MTextTheme].
  ///
  /// This factory constructor initializes the global styles using the
  /// provided color scheme and text theme.
  ///
  /// - [colors]: The color scheme to use for the global styles.
  /// - [textTheme]: The text theme to use for the global styles.
  ///
  /// Returns a new [MGlobalStyles] instance with the default styles applied.
  factory $default(MColorScheme colors, MTextTheme textTheme) {
    return MGlobalStyles(
      dividerColor: colors.outlineVariant1,
      checkboxTheme: CheckboxThemeData(
        shape: const RoundedRectangleBorder(borderRadius: Corners.xs),
        side: BorderSide(color: colors.outlineVariant1),
        materialTapTargetSize: MaterialTapTargetSize.shrinkWrap,
      ),
      snackBarTheme: SnackBarThemeData(
        contentTextStyle: textTheme.captionRegular,
        insetPadding: const EdgeInsets.all(Insets.lg),
        behavior: SnackBarBehavior.fixed,
        shape: const RoundedRectangleBorder(borderRadius: Corners.md),
      ),
    );
  }

  /// The color of the dividers.
  final Color dividerColor;

  /// The theme for SnackBars.
  final SnackBarThemeData snackBarTheme;

  /// The theme for checkboxes.
  final CheckboxThemeData checkboxTheme;

  /// Provides the [DividerThemeData] based on the current global styles.
  ///
  /// This getter constructs a [DividerThemeData] using the properties
  /// defined in the current instance of [MGlobalStyles].
  ///
  /// Returns a [DividerThemeData] instance with the styles applied.
  DividerThemeData get dividerTheme {
    return DividerThemeData(color: dividerColor, thickness: 1);
  }

  /// Retrieves the [MGlobalStyles] extension from the closest [Theme] instance
  /// that encloses the given [context].
  ///
  /// This method searches for the nearest [Theme] widget in the widget tree
  /// and returns the [MGlobalStyles] extension if it exists. If no
  /// [MGlobalStyles] extension is found, this method returns null.
  ///
  /// Example usage:
  /// ```dart
  /// final globalStyles = MGlobalStyles.of(context);
  /// ```
  ///
  /// - [context]: The build context from which to retrieve the [MGlobalStyles]
  /// extension.
  ///
  /// Returns the [MGlobalStyles] extension if found, or null if no
  /// [MGlobalStyles] extension is available in the closest [Theme] instance.
  static MGlobalStyles of(BuildContext context) {
    return Theme.of(context).extension<MGlobalStyles>()!;
  }

  @override
  ThemeExtension<MGlobalStyles> copyWith({
    MColor? dividerColor,
    SnackBarThemeData? snackBarTheme,
    CheckboxThemeData? checkBoxTheme,
  }) {
    return MGlobalStyles(
      dividerColor: dividerColor ?? this.dividerColor,
      snackBarTheme: snackBarTheme ?? this.snackBarTheme,
      checkboxTheme: checkBoxTheme ?? checkboxTheme,
    );
  }

  @override
  ThemeExtension<MGlobalStyles> lerp(
    ThemeExtension<MGlobalStyles>? other,
    double t,
  ) {
    if (other is! MGlobalStyles) return this;
    return MGlobalStyles(
      dividerColor: Color.lerp(dividerColor, other.dividerColor, t)!,
      checkboxTheme: CheckboxThemeData.lerp(
        checkboxTheme,
        other.checkboxTheme,
        t,
      ),
      snackBarTheme: SnackBarThemeData.lerp(
        snackBarTheme,
        other.snackBarTheme,
        t,
      ),
    );
  }
}
