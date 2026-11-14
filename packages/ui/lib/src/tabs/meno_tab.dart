import 'package:material_ui/material_ui.dart';

/// {@template meno_tab}
/// Data class to hold information for a tab (doesn't need to be a Widget).
///
/// This class provides a simple way to represent the data associated with
/// a tab, such as its text, optional icon, and an optional key.
///
/// This separation makes the tab bar implementation more flexible and easier
/// to test.
/// {@endtemplate}
class MenoTab {
  /// Creates a new [MenoTab].
  const new({required this.text, this.icon, this.key});

  /// The text displayed on the tab.
  final String text;

  /// An optional icon to display on the tab.
  final Widget? icon;

  /// An optional key to identify this tab.
  final Key? key;
}
