import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// The empty state of a notes collection.
///
/// One widget for both tabs: the notes and folders states differed only by
/// their picture, their sentence and which add button they mounted, while the
/// layout was identical line for line.
class EmptyNotesWidget extends StatelessWidget {
  /// Creates the state from its [image], [message] and the [action] that adds
  /// the first item.
  const new({
    required this.image,
    required this.message,
    this.action,
    super.key,
  });

  /// The illustration, at the size the caller's asset draws itself.
  final Widget image;

  /// The one sentence the state says.
  final String message;

  /// The button that adds the first item, or null when the screen already has
  /// one.
  final Widget? action;

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);

    return Center(
      child: SizedBox(
        width: 266,
        child: Column(
          children: [
            if (action != null) Spaces.verticalXLarge,
            image,
            MText(message, style: textTheme.bodyRegular, textAlign: .center),
            Spaces.verticalXLarge,
            ?action,
          ],
        ),
      ),
    );
  }
}
