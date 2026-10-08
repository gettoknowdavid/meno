import 'dart:ui';
import 'package:flutter/material.dart';


/// {@template tab_bar_style}
/// Custom styles for [MTabBar]
/// {@endtemplate}
class TabBarStyle {
  /// {@macro tab_bar_style}
  TabBarStyle({
    required this.labelColor,
    required this.backgroundColor,
    required this.dividerColor,
    required this.indicator,
    required this.dividerHeight,
    required this.iconTheme,
  });

  /// Selected color
  final WidgetStateProperty<Color?> labelColor;

  /// Unselected color
  final WidgetStateProperty<Color?> backgroundColor;

  /// Divider color
  final Color dividerColor;

  /// Height of divider
  final double dividerHeight;

  /// Tab indicator decoration
  final Decoration indicator;

  /// [IconThemeData] for the icon
  final WidgetStateProperty<IconThemeData?> iconTheme;

  /// CopyWith method
  TabBarStyle copyWith({
    WidgetStateProperty<Color?>? labelColor,
    WidgetStateProperty<Color?>? backgroundColor,
    Color? dividerColor,
    Decoration? indicator,
    double? dividerHeight,
    WidgetStateProperty<IconThemeData?>? iconTheme,
  }) {
    return TabBarStyle(
      labelColor: labelColor ?? this.labelColor,
      backgroundColor: backgroundColor ?? this.backgroundColor,
      dividerColor: dividerColor ?? this.dividerColor,
      indicator: indicator ?? this.indicator,
      dividerHeight: dividerHeight ?? this.dividerHeight,
      iconTheme: iconTheme ?? this.iconTheme,
    );
  }

  /// Linearly interpolates between two [TabBarStyle]s.
  static TabBarStyle? lerp(TabBarStyle? a, TabBarStyle? b, double t) {
    if (identical(a, b)) return a;
    return TabBarStyle(
      labelColor:
          WidgetStateProperty.lerp<Color?>(
            a?.labelColor,
            b?.labelColor,
            t,
            Color.lerp,
          )!,
      backgroundColor:
          WidgetStateProperty.lerp<Color?>(
            a?.labelColor,
            b?.labelColor,
            t,
            Color.lerp,
          )!,
      dividerColor: Color.lerp(a?.dividerColor, b?.dividerColor, t)!,
      indicator: Decoration.lerp(a?.indicator, b?.indicator, t)!,
      dividerHeight: lerpDouble(a?.dividerHeight, b?.dividerHeight, t)!,
      iconTheme:
          WidgetStateProperty.lerp<IconThemeData?>(
            a?.iconTheme,
            b?.iconTheme,
            t,
            IconThemeData.lerp,
          )!,
    );
  }
}
