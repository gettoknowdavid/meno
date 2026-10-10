import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/text/m_text.dart';
import 'package:meno_ui/src/theme/extensions/m_color_scheme.dart';
import 'package:meno_ui/src/theme/extensions/m_text_theme.dart';
import 'package:meno_ui/src/tokens/spacing.dart';

/// A sentence with a tappable call to action at its end.
///
/// The "Don't have an account? Create one" pattern: a statement in the muted
/// text style and an action in the brand colour, on one line. The action is a
/// whole phrase rather than a lone word, so the tap target is comfortably
/// larger than the minimum.
///
/// Example usage:
/// ```dart
/// MInlineActionText(
///   label: 'Already have an account?',
///   actionLabel: 'Log in',
///   onAction: goToLogin,
/// )
/// ```
class MInlineActionText extends StatelessWidget {
  /// Creates an inline action sentence.
  ///
  /// [label] is the statement, [actionLabel] the tappable phrase that ends it,
  /// and [onAction] what the tap does. A null [onAction] renders the action as
  /// plain text, which is how a screen shows the sentence while the action is
  /// unavailable.
  const new({
    required this.label,
    required this.actionLabel,
    super.key,
    this.onAction,
    this.mainAxisAlignment = MainAxisAlignment.center,
  });

  /// The statement, in the muted style.
  final String label;

  /// The tappable phrase that ends [label].
  final String actionLabel;

  /// Called when [actionLabel] is tapped.
  final VoidCallback? onAction;

  /// How the pair is distributed across the line.
  final MainAxisAlignment mainAxisAlignment;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return Row(
      mainAxisAlignment: mainAxisAlignment,
      children: [
        MText(label, style: textTheme.captionMedium),
        Spaces.horizontalMicro,
        InkWell(
          onTap: onAction,
          child: MText(
            actionLabel,
            style: textTheme.captionMedium,
            color: colors.primary,
          ),
        ),
      ],
    );
  }
}
