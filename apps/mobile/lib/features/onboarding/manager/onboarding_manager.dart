import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/features/onboarding/services/onboarding_service.dart';

/// Owns the flag that decides whether the intro still needs to be shown.
///
/// The persisted value is read in the constructor, so [isOnboarded] is already
/// correct when the manager resolves — the router guard can read it
/// synchronously without a separate initialise step to remember.
final class OnboardingManager extends ChangeNotifier implements Disposable {
  /// Creates a manager backed by [_service].
  new(this._service) {
    _isOnboarded.value = _service.isOnboarded;
  }

  final OnboardingService _service;
  final _isOnboarded = ValueNotifier<bool>(false);

  /// Whether the user has already seen and acted on the intro.
  ValueListenable<bool> get isOnboarded => _isOnboarded;

  /// Marks the intro as seen.
  ///
  /// [isOnboarded] flips first so the guard stops redirecting immediately, then
  /// the flag is written through to storage.
  late final Command<void, void> completeOnboarding =
      Command.createAsyncNoParamNoResult(() async {
        if (_isOnboarded.value) return;
        _isOnboarded.value = true;
        notifyListeners();
        await _service.completeOnboarding();
      });

  @override
  FutureOr<dynamic> onDispose() {
    _isOnboarded.dispose();
    completeOnboarding.dispose();
  }
}
