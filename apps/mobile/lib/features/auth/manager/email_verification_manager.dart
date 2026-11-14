import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/auth/manager/auth_manager.dart';

/// The resend-code cooldown for the email verification screen.
const Duration _resendTick = Duration(seconds: 1);
const Duration _resendCooldown = Duration(minutes: 1);

/// Owns the one action that sends a verification code: the user's tap.
///
/// Availability ([canResend]) is a *derived value* — the cooldown countdown
/// crossed with the running state — and never a trigger. Nothing here watches
/// it, so the countdown reaching zero sends nothing (M01); the earlier design,
/// where a `canResend` handler fired the command, could not know the
/// difference between "available again" and "the user asked".
///
/// The cooldown starts when the tap's command is *dispatched*, not when it
/// succeeds: the timer rate-limits the link, and a throttled or failed attempt
/// must re-apply it too — with one fixed constant, not a Retry-After parse.
final class EmailVerificationManager implements Disposable {
  /// Creates a manager over the auth command that sends a fresh code.
  new({required this._auth, this._clock = const SystemClock()}) {
    resend = Command.createAsyncNoResult<Email>(
      _resend,
      errorFilterFn: menoExceptionFilter,
      debugName: 'emailVerification.resend',
    );
  }

  final AuthManager _auth;
  final Clock _clock;

  Timer? _ticker;
  DateTime? _cooldownDeadline;

  /// Sends a fresh code, gated on the cooldown. The only sender in the app.
  late final Command<Email, void> resend;

  /// The seconds left before "Send again" is clickable again.
  ///
  /// Zero means the link is usable. A positive value means the countdown is
  /// running.
  late final ValueNotifier<int> _remaining = ValueNotifier(0);

  /// Whether a resend command is running right now.
  late final ValueListenable<bool> isResending = _auth.resendOtp.isRunning;

  /// Whether the page can offer the resend link.
  ///
  /// Built once and retained — never inside a `build` — so every subscriber
  /// watches the same notifier (L02). Changing it only re-evaluates; nothing
  /// listens in order to send.
  late final ValueListenable<bool> canResend = _remaining.select(
    (remaining) => remaining <= 0 && !isResending.value,
  );

  /// The seconds remaining in the current cooldown.
  ValueListenable<int> get remaining => _remaining;

  /// Starts the cooldown as of now.
  void startCooldown() => _setDeadline(_clock.nowUtc().add(_resendCooldown));

  /// Starts the cooldown from when the API actually sent a code.
  ///
  /// Registration mints the first code without a tap; that receipt is what the
  /// countdown runs from, so the screen opens with the remaining time rather
  /// than offering an immediate resend.
  void initializeFromReceipt(DateTime sentAt) {
    final deadline = sentAt.toUtc().add(_resendCooldown);
    if (deadline.isBefore(_clock.nowUtc())) {
      _cooldownDeadline = null;
      _remaining.value = 0;
      return;
    }
    _setDeadline(deadline);
  }

  @override
  FutureOr<dynamic> onDispose() {
    _ticker?.cancel();
    _ticker = null;
    resend.dispose();
    _remaining.dispose();
  }

  /// The tap: gated, cooldown-first, then dispatch to the transport.
  Future<void> _resend(Email email) async {
    if (!email.isValid || !canResend.value) return;

    startCooldown();
    _auth.resendOtp.run(OtpRequestArgs(email: email, type: .verifyEmail));
  }

  void _setDeadline(DateTime deadline) {
    _cooldownDeadline = deadline;
    _ticker?.cancel();
    _ticker = null;
    _applyRemaining();
    if (_remaining.value > 0) {
      _ticker = Timer.periodic(_resendTick, (_) => _applyRemaining());
    }
  }

  /// Recomputes the countdown from the deadline and the clock.
  ///
  /// Reading the deadline rather than counting down a number is what lets a
  /// resumed app pick up where it left off instead of restarting the minute.
  void _applyRemaining() {
    final deadline = _cooldownDeadline;
    if (deadline == null) {
      _remaining.value = 0;
      return;
    }

    final seconds = deadline.difference(_clock.nowUtc()).inSeconds;
    final next = seconds > 0 ? seconds : 0;
    if (next == 0) {
      _cooldownDeadline = null;
      _ticker?.cancel();
      _ticker = null;
    }
    if (_remaining.value != next) _remaining.value = next;
  }
}
