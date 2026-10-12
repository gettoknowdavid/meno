import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/features/auth/manager/auth_manager.dart';

/// The resend-code cooldown for the email verification screen.

/// A helper used by tests, so the cooldown duration and the timer tick is the
/// same as the production one without depending on clock drift across the
/// test run.
///
/// This is internal to the feature: it is the timer's step, not a public
/// configuration API for the cooldown length.
const Duration _resendTick = Duration(seconds: 1);
const Duration _resendCooldown = Duration(minutes: 1);

final class EmailVerificationManager implements Disposable {
  /// Creates a manager over the auth command that sends a fresh code.
  new({required this._auth});

  final AuthManager _auth;

  /// The seconds left before "Send again" is clickable again.
  ///
  /// Zero means the link is usable. A positive value means the countdown is
  /// running.
  late final ValueNotifier<int> _remaining = ValueNotifier(0);

  /// Whether a resend command is running right now.
  late final ValueListenable<bool> isResending = _auth.resendOtp.isRunning;

  /// Whether the page can offer the resend link.
  ///
  /// It is offered when the cooldown is finished and no resend is in flight.
  late final ValueListenable<bool> canResend = _remaining.select(
    (remaining) => remaining <= 0 && !isResending.value,
  );

  /// The seconds remaining in the current cooldown.
  ///
  /// Exposed so the page can render the countdown without watching the timer.
  ValueListenable<int> get remaining => _remaining;

  /// Starts the one-minute cooldown.
  ///
  /// The cooldown is started when a resend is requested — either because the
  /// user tapped the link or because the command was already running when the
  /// screen recomposed. It is not started by a successful verification; that
  /// flow leaves the screen.
  void startCooldown() {
    _remaining.value = _resendCooldown.inSeconds;
    _timer?.cancel();
    _timer = Timer.periodic(_resendTick, _tick);
  }

  @override
  FutureOr<dynamic> onDispose() {
    _timer?.cancel();
    _remaining.dispose();
  }

  Timer? _timer;

  void _tick(Timer timer) {
    final next = _remaining.value - _resendTick.inSeconds;
    if (next <= 0) {
      _remaining.value = 0;
      timer.cancel();
    } else {
      _remaining.value = next;
    }
  }
}
