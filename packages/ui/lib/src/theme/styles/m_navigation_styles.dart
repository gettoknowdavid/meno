import 'package:flutter/material.dart';
import 'package:meno_ui/src/styles.dart';
import 'package:meno_ui/src/theme/m_color.dart';
import 'package:meno_ui/src/theme/m_color_scheme.dart';
import 'package:meno_ui/src/theme/styles/m_text_theme.dart';

/// A theme extension for customizing the navigation elements in the
/// Meno design system.
///
/// This class defines various styles for navigation elements such as
/// AppBar, BottomNavigationBar, TabBar, and more.
///
/// Example usage:
/// ```dart
/// final navigationStyles = MNavigationStyles.of(context);
/// ```

class MNavigationStyles extends ThemeExtension<MNavigationStyles> {
  /// Creates a new instance of [MNavigationStyles].
  ///
  /// The constructor allows you to specify custom styles for navigation
  /// elements.
  ///
  /// - [appBarTheme]: The theme for the AppBar.
  /// - [bottomNavigationBarTheme]: The theme for the BottomNavigationBar.
  /// - [tabBarTheme]: The theme for the TabBar.
  /// - [accentColor]: The accent color for navigation elements.
  /// - [actionTextStyle]: The text style for action elements in navigation.
  const new({
    required this.appBarTheme,
    required this.bottomNavigationBarTheme,
    required this.tabBarTheme,
    required this.accentColor,
    required this.actionTextStyle,
  });

  /// Creates a default [MNavigationStyles] based on the given [MColorScheme]
  /// and [MTextTheme].
  ///
  /// This factory constructor initializes the navigation styles using the
  /// provided color scheme and text theme.
  ///
  /// - [colors]: The color scheme to use for the navigation styles.
  /// - [textTheme]: The text theme to use for the navigation styles.
  ///
  /// Returns a new [MNavigationStyles] instance with the default styles
  /// applied.
  factory $default(MColorScheme colors, MTextTheme textTheme) {
    return MNavigationStyles(
      appBarTheme: AppBarTheme(
        elevation: 0,
        scrolledUnderElevation: 0,
        shadowColor: Colors.transparent,
        backgroundColor: colors.background,
        toolbarHeight: 56,
        titleTextStyle: textTheme.bodyMedium,
        actionsIconTheme: IconThemeData(color: colors.onBackground, size: 24),
      ),
      bottomNavigationBarTheme: BottomNavigationBarThemeData(
        showSelectedLabels: true,
        showUnselectedLabels: true,
        backgroundColor: colors.background,
        unselectedItemColor: colors.inActive,
        selectedItemColor: colors.primary,
        selectedIconTheme: IconThemeData(color: colors.primary, size: 20),
        unselectedIconTheme: IconThemeData(color: colors.inActive, size: 20),
        unselectedLabelStyle: textTheme.microMedium.copyWith(
          color: colors.inActive,
        ),
        selectedLabelStyle: textTheme.microMedium.copyWith(
          color: colors.primary,
        ),
      ),
      accentColor: MColor.secondary300,
      actionTextStyle: textTheme.captionMedium,
      tabBarTheme: TabBarThemeData(
        labelStyle: textTheme.captionMedium,
        labelColor: colors.primary,
        labelPadding: const EdgeInsets.symmetric(horizontal: Insets.sm),
        unselectedLabelStyle: textTheme.captionMedium,
        unselectedLabelColor: colors.onBackgroundVariant,
        indicatorColor: colors.primary,
        indicatorSize: TabBarIndicatorSize.tab,
        indicator: BoxDecoration(
          border: Border(bottom: BorderSide(color: colors.primary)),
        ),
      ),
    );
  }

  /// The theme for the AppBar.
  final AppBarTheme appBarTheme;

  /// The theme for the BottomNavigationBar.
  final BottomNavigationBarThemeData bottomNavigationBarTheme;

  /// The theme for the TabBar.
  final TabBarThemeData tabBarTheme;

  /// The accent color for navigation elements.
  final MColor accentColor;

  /// The text style for action elements in navigation.
  final TextStyle actionTextStyle;

  /// Retrieves the [MNavigationStyles] extension from the closest [Theme]
  /// instance that encloses the given [context].
  ///
  /// This method searches for the nearest [Theme] widget in the widget tree
  /// and returns the [MNavigationStyles] extension if it exists. If no
  /// [MNavigationStyles] extension is found, this method returns null.
  ///
  /// Example usage:
  /// ```dart
  /// final navigationStyles = MNavigationStyles.of(context);
  /// ```
  ///
  /// - [context]: The build context from which to retrieve the
  /// [MNavigationStyles] extension.
  ///
  /// Returns the [MNavigationStyles] extension if found, or null if no
  /// [MNavigationStyles] extension is available in the closest [Theme]
  /// instance.
  static MNavigationStyles of(BuildContext context) {
    return Theme.of(context).extension<MNavigationStyles>()!;
  }

  @override
  ThemeExtension<MNavigationStyles> copyWith({
    AppBarTheme? appBarTheme,
    BottomNavigationBarThemeData? bottomNavigationBarTheme,
    TabBarThemeData? tabBarTheme,
    MColor? accentColor,
    TextStyle? actionTextStyle,
  }) {
    return MNavigationStyles(
      appBarTheme: appBarTheme ?? this.appBarTheme,
      tabBarTheme: tabBarTheme ?? this.tabBarTheme,
      accentColor: accentColor ?? this.accentColor,
      actionTextStyle: actionTextStyle ?? this.actionTextStyle,
      bottomNavigationBarTheme:
          bottomNavigationBarTheme ?? this.bottomNavigationBarTheme,
    );
  }

  @override
  ThemeExtension<MNavigationStyles> lerp(
    ThemeExtension<MNavigationStyles>? other,
    double t,
  ) {
    if (other is! MNavigationStyles) return this;
    return MNavigationStyles(
      appBarTheme: AppBarTheme.lerp(appBarTheme, other.appBarTheme, t),
      tabBarTheme: TabBarThemeData.lerp(tabBarTheme, other.tabBarTheme, t),
      accentColor: MColor.lerp(accentColor, other.accentColor, t)!,
      actionTextStyle: TextStyle.lerp(
        actionTextStyle,
        other.actionTextStyle,
        t,
      )!,
      bottomNavigationBarTheme: BottomNavigationBarThemeData.lerp(
        bottomNavigationBarTheme,
        other.bottomNavigationBarTheme,
        t,
      ),
    );
  }
}
