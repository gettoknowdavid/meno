import 'package:flutter/foundation.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// The dot strip under the carousel: one dot per slide, the current one larger
/// and coloured with the primary colour.
class OnboardingIndicator extends StatelessWidget {
  /// Creates the indicator for [itemsLength] slides.
  const new({required this.currentIndex, required this.itemsLength, super.key});

  /// The slide that is currently on screen.
  final int currentIndex;

  /// How many slides the carousel holds.
  final int itemsLength;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    return Center(
      child: SizedBox(
        height: Insets.sm,
        child: Wrap(
          spacing: Insets.xs,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: List.generate(itemsLength, (index) {
            final selected = currentIndex == index;
            final dimensions = selected ? Insets.sm : Insets.xs;
            return Container(
              height: dimensions,
              width: dimensions,
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                color: selected ? colors.primary : colors.outlineVariant3,
              ),
            );
          }),
        ),
      ),
    );
  }

  @override
  void debugFillProperties(DiagnosticPropertiesBuilder properties) {
    super.debugFillProperties(properties);
    properties
      ..add(IntProperty('currentIndex', currentIndex))
      ..add(IntProperty('itemsLength', itemsLength));
  }
}
