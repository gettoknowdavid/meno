import 'package:flutter/foundation.dart';
import 'package:meno_ui/meno_ui.dart';

/// The slides shown by the intro carousel, in order.
///
/// The illustrations ship inside `meno_ui`, so each item carries the generated
/// asset rather than a string path — the widget that renders it then supplies
/// the package prefix in one place.
final List<OnboardingItem> onboardingItems = [
  OnboardingItem(
    title: 'Broadcast',
    subtitle:
        'to your audience and share links for them to join your broadcast.',
    image: Assets.images.onboarding1,
  ),
  OnboardingItem(
    title: 'Stream',
    subtitle:
        'broadcasts from individuals or ministries you are interested in.',
    image: Assets.images.onboarding2,
  ),
  OnboardingItem(
    title: 'Live Bible',
    subtitle: 'as you stream, have an immersive experience.',
    image: Assets.images.onboarding3,
  ),
  OnboardingItem(
    title: 'Take Notes',
    subtitle: 'as you stream, take note of things that stand out to you.',
    image: Assets.images.onboarding4,
  ),
];

/// One slide of the intro carousel.
@immutable
final class OnboardingItem {
  /// Creates a slide with [title], [subtitle] and its illustration.
  const new({required this.title, required this.subtitle, required this.image});

  /// The short headline, emphasised by the underlined title widget.
  final String title;

  /// The supporting line under the headline.
  final String subtitle;

  /// The illustration bundled with `meno_ui`.
  final AssetGenImage image;
}
