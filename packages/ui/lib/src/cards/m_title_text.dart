import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// A broadcast's title as the cards and live screens draw it.
///
/// One line, centred by default, ellipsised rather than wrapped: the title sits
/// above an artwork and under a header, so a second line would push the rest of
/// the card out of place. The caller that wants a fixed height wraps it in a
/// [SizedBox], because how much room a title gets is the screen's decision.
///
/// ```dart
/// MTitleText(title: broadcast.title)
/// ```
class MTitleText extends StatelessWidget {
  /// Creates a broadcast title.
  const new({
    required this.title,
    super.key,
    this.maxLines = 1,
    this.textAlign = .center,
  });

  /// The title to draw.
  final String title;

  /// How many lines the title may take before it is ellipsised.
  final int maxLines;

  /// How the title is aligned inside the space it is given.
  final TextAlign textAlign;

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);

    return MText(
      title,
      style: textTheme.subheadingMedium,
      textAlign: textAlign,
      maxLines: maxLines,
      overflow: .ellipsis,
    );
  }
}
