//
// ignore_for_file: avoid_positional_boolean_parameters

import 'package:flutter/material.dart';

/// Provides helper functions for design system
class MInternal {
  /// Resolves the appropriate value based on the theme brightness.
  ///
  /// This method selects either the light theme value or the dark theme value
  /// based on whether the theme brightness is light or dark.
  ///
  /// - [isLight]: A boolean indicating if the theme is light.
  /// - [lightThemeValue]: The value to use for light themes.
  /// - [darkThemeValue]: The value to use for dark themes.
  ///
  /// Returns the appropriate value for the current theme brightness.
  static T resolve<T>(bool isLight, T lightThemeValue, T darkThemeValue) {
    return isLight ? lightThemeValue : darkThemeValue;
  }

  /// Convenience method for easier use of [WidgetStateProperty.all].
  static WidgetStateProperty<T> all<T>(T value) {
    return WidgetStateProperty.all(value);
  }

  /// Convenience method for easier use of [WidgetStateProperty.resolveWith].
  static WidgetStateProperty<T?> resolveWith<T>({
    required T defaultValue,
    T? pressedValue,
    T? disabledValue,
    T? hoveredValue,
    String? parent,
    T? selectedValue,
  }) {
    return WidgetStateProperty.resolveWith((states) {
      // disabled
      if (states.contains(WidgetState.disabled) && disabledValue != null) {
        return disabledValue;
      }

      // pressed / focused
      if (states.any({WidgetState.pressed, WidgetState.focused}.contains) &&
          pressedValue != null) {
        return pressedValue;
      }
      // hovered
      if (states.contains(WidgetState.hovered) && hoveredValue != null) {
        return hoveredValue;
      }

      // selected
      if (states.contains(WidgetState.selected) && selectedValue != null) {
        return selectedValue;
      }
      // default
      return defaultValue;
    });
  }
}

/// Extension helper to get the values for [WidgetStateProperty]s
extension WidgetStatePropertyX<T> on WidgetStateProperty<T> {
  /// Disabled value
  T get disabledValue => resolve({WidgetState.disabled});

  /// Dragged value
  T get draggedValue => resolve({WidgetState.dragged});

  /// Error value
  T get errorValue => resolve({WidgetState.error});

  /// Focused value
  T get focusedValue => resolve({WidgetState.focused});

  /// Hovered value
  T get hoveredValue => resolve({WidgetState.hovered});

  /// Pressed value
  T get pressedValue => resolve({WidgetState.pressed});

  /// Selected value
  T get selectedValue => resolve({WidgetState.selected});

  /// Scrolled Under value
  T get scrolledUnderValue => resolve({WidgetState.scrolledUnder});

  /// None value
  T get noneValue => resolve({});
}
