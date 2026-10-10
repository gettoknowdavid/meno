import 'package:flutter/foundation.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// The headline of an intro slide: the title with a yellow highlight rule drawn
/// across its full width, which is why the text sits in a [Stack].
class OnboardingTitle extends StatelessWidget {
  /// Creates the title that renders [text].
  const new(this.text, {super.key});

  /// The headline to render.
  final String text;

  @override
  Widget build(BuildContext context) => Center(
    child: SizedBox(
      height: _titleHeight,
      child: LayoutBuilder(
        builder: (context, constraints) => Stack(
          children: [
            MText(
              text,
              style: MTextTheme.of(context).heading1Bold,
              textAlign: TextAlign.center,
            ),
            Positioned(
              bottom: 0,
              height: Insets.sm,
              width: constraints.maxWidth,
              child: const ColoredBox(color: MColor.decorativeYellow75),
            ),
          ],
        ),
      ),
    ),
  );

  @override
  void debugFillProperties(DiagnosticPropertiesBuilder properties) {
    super.debugFillProperties(properties);
    properties.add(StringProperty('text', text));
  }
}

/// Fixed height reserved for a slide headline.
const double _titleHeight = 44;
