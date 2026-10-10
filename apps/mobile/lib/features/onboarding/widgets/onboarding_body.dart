import 'package:flutter/foundation.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/onboarding/model/onboarding_item.dart';
import 'package:meno_mobile/features/onboarding/widgets/onboarding_subtitle.dart';
import 'package:meno_mobile/features/onboarding/widgets/onboarding_title.dart';
import 'package:meno_ui/meno_ui.dart';

/// The illustration, headline and supporting line of one intro slide.
class OnboardingBody extends StatelessWidget {
  /// Creates the body that renders [item].
  const new(this.item, {super.key});

  /// The slide to render.
  final OnboardingItem item;

  @override
  Widget build(BuildContext context) {
    const imageDimension = 240.0;
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: Insets.lg),
      child: Column(
        children: [
          // The illustration ships inside `meno_ui`, so the package prefix has
          // to be passed along with the generated asset name.
          item.image.image(
            height: imageDimension,
            width: imageDimension,
            package: 'meno_ui',
          ),
          Spaces.verticalXLarge,
          OnboardingTitle(item.title),
          Spaces.verticalLarge,
          Flexible(child: OnboardingSubtitle(item.subtitle)),
        ],
      ),
    );
  }

  @override
  void debugFillProperties(DiagnosticPropertiesBuilder properties) {
    super.debugFillProperties(properties);
    properties.add(DiagnosticsProperty<OnboardingItem>('item', item));
  }
}
