import 'package:flutter/foundation.dart';

/// Tracks whether the user has completed first-run onboarding.
///
/// Exposed as a [ValueNotifier] so the router guard and reactive widgets share
/// a single source of truth.
final class OnboardingManager {
  /// Whether onboarding has been completed.
  final ValueNotifier<bool> isOnboarded = ValueNotifier<bool>(false);

  /// Marks onboarding as completed.
  void complete() {
    isOnboarded.value = true;
  }

  /// Resets onboarding to its initial state.
  void reset() {
    isOnboarded.value = false;
  }
}
