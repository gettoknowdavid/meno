import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/settings/model/_model.dart';

/// The theme the app is currently painted in.
///
/// It holds the account's display preference as a [ThemeMode] so the widget
/// that owns the [MaterialApp] can watch it. It is registered eagerly and in
/// the root scope, which is the point: the app's first frame and the loading
/// screens shown before the container is ready are already drawn in the theme
/// the last account chose, instead of flashing the system one.
final class ThemeManager {
  /// Creates a manager following the system theme.
  new();

  /// The mode [MaterialApp] should use.
  final ValueNotifier<ThemeMode> themeMode = ValueNotifier(ThemeMode.system);

  /// Adopts an account's display preference.
  void apply(UserDisplay display) {
    themeMode.value = switch (display) {
      UserDisplay.system => ThemeMode.system,
      UserDisplay.light => ThemeMode.light,
      UserDisplay.dark => ThemeMode.dark,
    };
  }
}
