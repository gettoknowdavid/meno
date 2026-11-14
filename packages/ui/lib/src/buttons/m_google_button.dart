import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/buttons/m_secondary_button.dart';
import 'package:meno_ui/src/gen/assets.gen.dart';
import 'package:meno_ui/src/theme/extensions/m_color_scheme.dart';
import 'package:meno_ui/src/tokens/strokes.dart';

/// A secondary-style button carrying the Google mark.
///
/// Example usage:
/// ```dart
/// MGoogleButton(
///   title: 'Sign in with Google',
///   onPressed: signInWithGoogle,
/// );
/// ```
class MGoogleButton extends StatelessWidget {
  /// Creates a Google button.
  ///
  /// Parameters:
  /// - [title]: The label displayed on the button.
  /// - [key]: An optional key to identify the widget.
  /// - [onPressed]: The callback invoked when the button is pressed.
  const new({required this.title, super.key, this.onPressed});

  /// The label displayed on the button.
  final String title;

  /// The callback invoked when the button is pressed.
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);

    return MSecondaryButton.icon(
      label: title,
      icon: Assets.images.google.svg(package: 'meno_ui'),
      onPressed: onPressed,
      style: OutlinedButton.styleFrom(
        side: BorderSide(width: Strokes.thin, color: colors.inActiveContainer),
        foregroundColor: colors.onBackground,
        backgroundColor: colors.background,
      ),
    );
  }
}
