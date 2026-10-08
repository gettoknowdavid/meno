import 'package:flutter/material.dart';
import 'package:meno_ui/src/gen/fonts.gen.dart';
import 'package:meno_ui/src/styles.dart';
import 'package:meno_ui/src/theme/m_color_scheme.dart';
import 'package:meno_ui/src/theme/styles/m_button_styles.dart';
import 'package:meno_ui/src/theme/styles/m_card_styles.dart';
import 'package:meno_ui/src/theme/styles/m_global_styles.dart';
import 'package:meno_ui/src/theme/styles/m_modal_styles.dart';
import 'package:meno_ui/src/theme/styles/m_navigation_styles.dart';
import 'package:meno_ui/src/theme/styles/m_otp_field_style.dart';
import 'package:meno_ui/src/theme/styles/m_tab_bar_theme.dart';
import 'package:meno_ui/src/theme/styles/m_text_field_style.dart';
import 'package:meno_ui/src/theme/styles/m_text_theme.dart';

/// A utility class for creating and accessing theme data based on the
/// application's brightness setting.
///
/// The [MTheme] class provides static methods to obtain theme data for both
/// dark and light modes. This class is intended to simplify the process of
/// configuring and accessing themes throughout the application.
///
/// Example usage:
/// ```dart
/// ThemeData lightTheme = MTheme.light;
/// ThemeData darkTheme = MTheme.dark;
/// ```
class MTheme {
  // Private constructor to prevent instantiation.
  new _();

  /// Retrieves the [ThemeData] for the specified [Brightness].
  ///
  /// This method returns a [ThemeData] instance configured according to the
  /// given [brightness]. It internally calls a private method to generate
  /// the appropriate theme data.
  ///
  /// - [brightness]: The brightness mode for which to retrieve the theme data.
  ///
  /// Returns the [ThemeData] instance configured for the specified brightness.
  static ThemeData theme(Brightness brightness) => _raw(brightness);

  /// Retrieves the [ThemeData] for dark mode.
  ///
  /// This getter provides a [ThemeData] instance configured for dark mode,
  /// using the internal method to generate the appropriate theme data.
  ///
  /// Returns the [ThemeData] instance configured for dark mode.
  static ThemeData get dark => _raw(Brightness.dark);

  /// Retrieves the [ThemeData] for light mode.
  ///
  /// This getter provides a [ThemeData] instance configured for light mode,
  /// using the internal method to generate the appropriate theme data.
  ///
  /// Returns the [ThemeData] instance configured for light mode.
  static ThemeData get light => _raw(Brightness.light);

  // Private method to generate raw theme data based on brightness.
  static ThemeData _raw(Brightness brightness) {
    final colorScheme = MColorScheme.$default(brightness);
    final textTheme = MTextTheme.$default(colorScheme);

    final buttonStyles = MButtonStyles.$default(colorScheme, textTheme);
    final cardStyles = MCardStyles.$default(colorScheme, textTheme);
    final globalStyles = MGlobalStyles.$default(colorScheme, textTheme);
    final modalStyles = MModalStyles.$default(colorScheme);
    final navStyles = MNavigationStyles.$default(colorScheme, textTheme);
    final otpStyles = MOtpFieldStyles.$default(colorScheme, textTheme);
    final textInputStyles = MTextFieldStyle.$default(colorScheme, textTheme);
    final tabBarTheme = MTabBarTheme.$default(colorScheme);

    return ThemeData(
      cardTheme: cardStyles.cardTheme,
      colorScheme: colorScheme.getColorScheme,
      elevatedButtonTheme: buttonStyles.elevatedButtonTheme,
      outlinedButtonTheme: buttonStyles.outlinedButtonTheme,
      textButtonTheme: buttonStyles.textButtonTheme,
      dividerTheme: globalStyles.dividerTheme,
      dividerColor: globalStyles.dividerColor,
      scaffoldBackgroundColor: colorScheme.background,
      bottomNavigationBarTheme: navStyles.bottomNavigationBarTheme,
      appBarTheme: navStyles.appBarTheme,
      tabBarTheme: navStyles.tabBarTheme,
      iconTheme: IconThemeData(color: colorScheme.onBackground, size: 24),
      fontFamily: FontFamily.sFProDisplay,
      disabledColor: colorScheme.disabled,
      useMaterial3: true,
      snackBarTheme: globalStyles.snackBarTheme,
      checkboxTheme: globalStyles.checkboxTheme,
      bottomSheetTheme: modalStyles.bottomSheetTheme,
      listTileTheme: ListTileThemeData(textColor: colorScheme.onBackground),
      progressIndicatorTheme: ProgressIndicatorThemeData(
        color: colorScheme.primary,
        linearMinHeight: 4,
        linearTrackColor: colorScheme.background,
        circularTrackColor: colorScheme.background,
      ),
      inputDecorationTheme: InputDecorationTheme(
        border: textInputStyles.border,
        focusedBorder: textInputStyles.borderFocused,
        enabledBorder: textInputStyles.border,
        errorBorder: textInputStyles.borderError,
        disabledBorder: textInputStyles.borderDisabled,
        filled: true,
        iconColor: textInputStyles.iconColor,
        hintStyle: textInputStyles.hintTextStyle,
        labelStyle: textInputStyles.labelTextStyle,
        errorStyle: textInputStyles.errorTextStyle,
        contentPadding: const EdgeInsets.symmetric(horizontal: Insets.md),
      ),
      chipTheme: ChipThemeData(
        showCheckmark: false,
        padding: const EdgeInsets.symmetric(horizontal: Insets.lg, vertical: 6),
        labelPadding: EdgeInsets.zero,
        side: BorderSide.none,
        shape: const RoundedRectangleBorder(borderRadius: Corners.circle),
        labelStyle: textTheme.captionMedium,
        color: WidgetStateProperty.resolveWith((states) {
          if (states.contains(WidgetState.selected)) {
            return colorScheme.primary;
          } else {
            return colorScheme.inActiveContainer;
          }
        }),
      ),
      navigationBarTheme: NavigationBarThemeData(
        backgroundColor: colorScheme.background,
        elevation: 0,
        indicatorColor: Colors.transparent,
        labelBehavior: NavigationDestinationLabelBehavior.alwaysShow,
        iconTheme: WidgetStateProperty.resolveWith((states) {
          if (states.contains(WidgetState.selected)) {
            return IconThemeData(size: 20, color: colorScheme.primary);
          }
          return IconThemeData(size: 20, color: colorScheme.inActive);
        }),
        labelTextStyle: WidgetStateTextStyle.resolveWith((states) {
          final style = textTheme.microMedium;
          if (states.contains(WidgetState.selected)) {
            return style.copyWith(color: colorScheme.primary);
          }
          return style.copyWith(color: colorScheme.inActive);
        }),
      ),
      navigationRailTheme: NavigationRailThemeData(
        backgroundColor: colorScheme.background,
        minWidth: 224,
        minExtendedWidth: 224,
        elevation: 0,
        indicatorColor: Colors.transparent,
        labelType: NavigationRailLabelType.none,
        selectedIconTheme: IconThemeData(size: 20, color: colorScheme.primary),
        unselectedIconTheme: IconThemeData(
          size: 20,
          color: colorScheme.inActive,
        ),
        selectedLabelTextStyle: textTheme.microMedium.copyWith(
          color: colorScheme.primary,
        ),
        unselectedLabelTextStyle: textTheme.microMedium.copyWith(
          color: colorScheme.inActive,
        ),
      ),
      extensions: [
        colorScheme,
        textTheme,
        buttonStyles,
        globalStyles,
        cardStyles,
        navStyles,
        modalStyles,
        textInputStyles,
        otpStyles,
        tabBarTheme,
      ],
    );
  }
}
