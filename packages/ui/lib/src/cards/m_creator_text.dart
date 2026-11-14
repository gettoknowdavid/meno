import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// A creator/host name display widget with a chevron indicator.
///
/// Displays a creator's name as a text button with a right chevron,
/// typically used for navigation to a creator's profile.
///
/// Example usage:
/// ```dart
/// MCreatorText(
///   name: 'John Doe',
///   onPressed: () => context.push(R.profile(userId)),
/// )
/// ```
class MCreatorText extends StatelessWidget {
  /// Creates an [MCreatorText] widget.
  const new({required this.name, super.key, this.onPressed});

  /// The creator's name to display.
  final String name;

  /// Called when the creator name is tapped.
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);

    return SizedBox(
      height: 16,
      child: MTextButton.icon(
        label: name,
        icon: const Icon(MIcons.chevron_right),
        iconPlacement: .right,
        style: TextButton.styleFrom(
          padding: .zero,
          foregroundColor: colors.onBackgroundVariant,
          iconColor: colors.onBackgroundVariant,
        ),
        onPressed: onPressed,
      ),
    );
  }
}
