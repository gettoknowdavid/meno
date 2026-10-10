import 'package:flutter/foundation.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// The supporting line of an intro slide, centred and capped at two lines so
/// every slide keeps the same height.
class OnboardingSubtitle extends StatelessWidget {
  /// Creates the subtitle that renders [text].
  const new(this.text, {super.key});

  /// The line to render.
  final String text;

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);
    return Center(
      child: Container(
        height: Insets.xxxl,
        padding: const EdgeInsets.symmetric(horizontal: _sidePadding),
        child: MText(
          text,
          style: textTheme.bodyRegular,
          textAlign: TextAlign.center,
          maxLines: 2,
        ),
      ),
    );
  }

  @override
  void debugFillProperties(DiagnosticPropertiesBuilder properties) {
    super.debugFillProperties(properties);
    properties.add(StringProperty('text', text));
  }
}

/// Inset that keeps the subtitle away from the screen edges.
const double _sidePadding = 30;
