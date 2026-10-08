// Kept private to the package: these helpers are not part of the public API.
// `resolve` mirrors the `bool isLight` convention used by Flutter itself.
//
// ignore_for_file: avoid_positional_boolean_parameters

import 'package:flutter/material.dart';

/// Shared helpers for the theme extensions.
///
/// Not exported from `package:meno_ui/meno_ui.dart`.
abstract final class MInternal {
  /// Resolves the appropriate value based on the theme brightness.
  ///
  /// - [isLight]: Whether the theme is light.
  /// - [lightThemeValue]: The value to use for light themes.
  /// - [darkThemeValue]: The value to use for dark themes.
  static T resolve<T>(bool isLight, T lightThemeValue, T darkThemeValue) {
    return isLight ? lightThemeValue : darkThemeValue;
  }

  /// Convenience wrapper around [WidgetStateProperty.all].
  static WidgetStateProperty<T> all<T>(T value) {
    return WidgetStateProperty.all(value);
  }

  /// Convenience wrapper around [WidgetStateProperty.resolveWith].
  ///
  /// Returns [defaultValue] for any state that has no explicit override, so
  /// callers only declare the states they actually restyle.
  static WidgetStateProperty<T> resolveWith<T>({
    required T defaultValue,
    T? pressedValue,
    T? disabledValue,
    T? hoveredValue,
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

/// Reads the value a [WidgetStateProperty] resolves to for a single state.
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
