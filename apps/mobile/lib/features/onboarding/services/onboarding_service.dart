import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';

/// Persists whether the intro has been seen on this install.
///
/// Reads are synchronous because [_storage] caches its values once the app's
/// startup gate has let it through.
final class OnboardingService {
  /// Creates a service over [_storage].
  const new(this._storage);

  final LocalStorage _storage;

  /// Whether the intro has already been completed on this install.
  bool get isOnboarded =>
      _storage.getBool(StorageKeys.onboardingCompleted) ?? false;

  /// Records that the intro has been completed.
  Future<void> completeOnboarding() =>
      _storage.setBool(StorageKeys.onboardingCompleted, value: true);
}
