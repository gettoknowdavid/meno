import 'package:shared_preferences/shared_preferences.dart';

/// Persists whether the intro has been seen on this install.
///
/// Reads are synchronous because [_preferences] caches its values once the
/// instance is created — which is why the instance is registered as an async
/// singleton, letting the app's startup gate wait for it.
final class OnboardingService {
  /// Creates a service over [_preferences].
  const new(this._preferences);

  /// The preference key holding the completed flag.
  static const String onboardingKey = 'onboarding.completed';

  final SharedPreferences _preferences;

  /// Whether the intro has already been completed on this install.
  bool get isOnboarded => _preferences.getBool(onboardingKey) ?? false;

  /// Records that the intro has been completed.
  Future<void> completeOnboarding() =>
      _preferences.setBool(onboardingKey, true);
}
