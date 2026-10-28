import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';

/// How long a broadcast has been on air.
///
/// # Why a monotonic anchor
///
/// The elapsed time is measured against [Clock.monotonic], never against the
/// wall clock. A device that corrects its time zone or syncs its clock mid
/// broadcast moves `now - startedAt` by hours; a monotonic reading only ever
/// moves forward at one second per second, so the clock on screen cannot jump
/// (audit L03). The API's own elapsed is applied once, by [reconcile], and from
/// then on the local anchor is the truth.
///
/// # Why plain methods
///
/// Starting, pausing and stopping the clock are orchestration inside a session,
/// not user actions, so they are methods rather than commands (audit M21).
/// Nothing here reports a failure or waits to be watched — a `Command` per
/// method only added a result nobody read.
///
/// The notifier is updated at most once a second: the ticker runs more often so
/// pausing and resuming do not lose a fraction, but a frame is only rebuilt
/// when the displayed second actually changes (audit L01's granularity).
final class BroadcastTimerManager implements Disposable {
  new({
    required this._clock,
    required this._logger,
    Duration initialElapsed = Duration.zero,
  }) : _accumulated = initialElapsed < Duration.zero
           ? Duration.zero
           : initialElapsed {
    elapsed.value = _accumulated;
  }

  final Clock _clock;
  final MenoLogger _logger;

  /// Time measured before the current run, and while paused.
  Duration _accumulated;

  /// When the running stretch began, on the monotonic clock.
  Duration? _segmentStart;

  /// When the current pause began, on the monotonic clock.
  Duration? _pausedAt;

  Duration _pausedTotal = Duration.zero;

  bool _running = false;

  /// Whether the API's elapsed has already been applied.
  bool _reconciled = false;

  Timer? _ticker;

  /// The last second published, so equal ticks do not notify.
  int _publishedSeconds = 0;

  /// How long the broadcast has been on air.
  final elapsed = ValueNotifier<Duration>(Duration.zero);

  /// Whether the clock is ticking.
  final ValueNotifier<bool> isRunning = ValueNotifier(false);

  /// [elapsed] as `HH:MM:SS`.
  late final ValueListenable<String> formattedTime = elapsed.map(_format);

  /// [elapsed] as `2h ago`, `5m ago` or `9s ago`.
  late final ValueListenable<String> timeAgo = elapsed.map(_ago);

  /// How long the broadcast has been on air.
  Duration get currentElapsed => elapsed.value;

  /// How long it was paused for in total, including a pause in progress.
  Duration get pausedTotal {
    final pausedAt = _pausedAt;
    if (pausedAt == null) return _pausedTotal;
    return _pausedTotal + (_clock.monotonic() - pausedAt);
  }

  /// [currentElapsed] as `HH:MM:SS`.
  String get formatted => _format(elapsed.value);

  /// How much of the broadcast was actually on air, as a percentage.
  ///
  /// 100 is a broadcast that never paused; the score falls as the paused share
  /// of the elapsed time rises, which is what a summary screen reports.
  double get qualityScore {
    final total = elapsed.value.inSeconds;
    if (total == 0) return 100;
    final pausedShare = (_pausedTotal.inSeconds / total) * 100;
    return (100 - pausedShare).clamp(0, 100);
  }

  /// Starts, or continues after a [pause].
  void start() {
    if (_running) return;
    _running = true;
    isRunning.value = true;

    // A pause ends here for good: its length is kept for [qualityScore], not
    // subtracted again by the running segment.
    final pausedAt = _pausedAt;
    if (pausedAt != null) {
      _pausedTotal += _clock.monotonic() - pausedAt;
      _pausedAt = null;
    }

    _segmentStart = _clock.monotonic();
    _publish(force: true);

    _ticker ??= Timer.periodic(
      const Duration(milliseconds: 250),
      (_) => _tick(),
    );
    _logger.d(
      'Timer: running from ${elapsed.value.inSeconds}s',
      tag: MenoLogTag.lifecycle.label,
    );
  }

  /// Holds the clock where it is. The time is kept, not discarded: it is what
  /// a dropped connection cost, and it is subtracted from elapsed.
  void pause() {
    if (!_running) return;
    _accumulate();
    _running = false;
    isRunning.value = false;
    _ticker?.cancel();
    _ticker = null;
    _pausedAt = _clock.monotonic();
    _publish();
  }

  /// Continues after a [pause].
  void resume() => start();

  /// Stops the clock without resetting what it measured.
  void stop() {
    if (!_running) {
      _ticker?.cancel();
      _ticker = null;
      return;
    }
    _accumulate();
    _running = false;
    isRunning.value = false;
    _ticker?.cancel();
    _ticker = null;
    _publish();
  }

  /// Stops and forgets everything measured.
  void reset() {
    _ticker?.cancel();
    _ticker = null;
    _segmentStart = null;
    _accumulated = Duration.zero;
    _pausedAt = null;
    _pausedTotal = Duration.zero;
    _reconciled = false;
    _running = false;
    isRunning.value = false;
    _publish(force: true);
  }

  /// Moves the elapsed time to the API's own figure, once.
  ///
  /// Applied at most once per session: a later answer would undo the time this
  /// device measured while the API's snapshot was in flight, and reconciliation
  /// exists to correct the starting point, not to keep correcting it. Rebased
  /// on the monotonic clock rather than assigned, so the run continues from the
  /// reconciled figure without a jump on the next tick.
  void reconcile(Duration serverElapsed) {
    if (_reconciled) return;
    _reconciled = true;

    _accumulated = serverElapsed < Duration.zero
        ? Duration.zero
        : serverElapsed;
    if (_running) {
      _segmentStart = _clock.monotonic();
    }
    _publish(force: true);
  }

  void _accumulate() {
    final segmentStart = _segmentStart;
    if (segmentStart != null) {
      _accumulated += _clock.monotonic() - segmentStart;
    }
    _segmentStart = null;

    final pausedAt = _pausedAt;
    if (pausedAt != null) {
      _pausedTotal += _clock.monotonic() - pausedAt;
      _pausedAt = null;
    }
  }

  void _tick() {
    if (!_running) return;
    _publish();
  }

  /// Recomputes [elapsed] now, without waiting for the next tick.
  ///
  /// The ticker runs on real time; this is how a session — or a test with a
  /// clock it controls — asks for the value at a chosen instant.
  void refresh() => _publish();

  /// Publishes [elapsed] when the displayed second changed.
  void _publish({bool force = false}) {
    final segmentStart = _segmentStart;
    final value = _running && segmentStart != null
        ? _accumulated + (_clock.monotonic() - segmentStart)
        : _accumulated;
    final seconds = value.inSeconds;
    if (!force && seconds == _publishedSeconds) return;
    _publishedSeconds = seconds;
    elapsed.value = value;
  }

  static String _format(Duration duration) {
    final hours = duration.inHours.toString().padLeft(2, '0');
    final minutes = (duration.inMinutes % 60).toString().padLeft(2, '0');
    final seconds = (duration.inSeconds % 60).toString().padLeft(2, '0');
    return '$hours:$minutes:$seconds';
  }

  static String _ago(Duration duration) {
    if (duration.inHours > 0) return '${duration.inHours}h ago';
    if (duration.inMinutes > 0) return '${duration.inMinutes}m ago';
    return '${duration.inSeconds}s ago';
  }

  @override
  FutureOr<dynamic> onDispose() {
    _ticker?.cancel();
    _ticker = null;

    elapsed.dispose();
    isRunning.dispose();
  }
}
