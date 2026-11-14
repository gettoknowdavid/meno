import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// A back button that names where it goes back to.
///
/// An editor that was pushed from a tab should say which tab — an unlabelled
/// chevron leaves the user guessing whether it returns to the list they came
/// from or closes the whole section. The label is the destination, not the
/// action.
class MCustomBackButton extends StatelessWidget {
  /// Creates the button, which goes back and shows [title].
  const new({required this.title, super.key, this.onTap, this.color});

  /// What the destination is called.
  final String title;

  /// What to do instead of popping. Defaults to popping the route.
  final VoidCallback? onTap;

  /// The colour of the arrow and the label. Defaults to the text colour.
  final Color? color;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);
    final foreground = color ?? colors.onBackground;

    return InkWell(
      onTap: onTap ?? () => context.pop(),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          SizedBox.square(
            dimension: 48,
            child: Icon(MIcons.chevron_left, size: 24, color: foreground),
          ),
          MText(title, style: textTheme.bodyMedium, color: foreground),
        ],
      ),
    );
  }
}
