import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';

/// How long a broadcast has been on air.
///
/// The clock is derived rather than accumulated: every tick recomputes
/// `now - start - paused`, so a suspended app, a missed tick or a device clock
/// change cannot make the timer drift the way counting seconds can. Pausing is
/// what the interruption cases need — a dropped connection, an incoming call —
/// and the time spent paused is subtracted rather than forgotten, so it also
/// answers how reliable the broadcast was.
final class BroadcastTimerManager implements Disposable {
  new({
    required this._broadcastStartTime,
    required this._logger,
    Duration? initialElapsed,
  }) : _initialElapsed = initialElapsed ?? Duration.zero {
    initialize = .createSyncNoParamNoResult(_initialize);
    start = .createSyncNoParamNoResult(_start);
    pause = .createSyncNoParamNoResult(_pause);
    resume = .createSyncNoParamNoResult(_resume);
    stop = .createSyncNoParamNoResult(_stop);
    reset = .createSyncNoParamNoResult(_reset);
  }

  final DateTime _broadcastStartTime;
  final Duration _initialElapsed;
  final MenoLogger _logger;

  Timer? _timer;
  DateTime? _pausedAt;
  Duration _pausedTotal = Duration.zero;

  /// How long the broadcast has been on air.
  final elapsed = ValueNotifier<Duration>(Duration.zero);

  /// Whether the clock is ticking.
  final ValueNotifier<bool> isRunning = ValueNotifier(false);

  /// [elapsed] as `HH:MM:SS`.
  late final ValueListenable<String> formattedTime = elapsed.map(_format);

  /// [elapsed] as `2h ago`, `5m ago` or `9s ago`.
  late final ValueListenable<String> timeAgo = elapsed.map(_ago);

  late final Command<void, void> initialize;
  late final Command<void, void> start;
  late final Command<void, void> pause;
  late final Command<void, void> resume;
  late final Command<void, void> stop;
  late final Command<void, void> reset;

  /// How long the broadcast has been on air.
  Duration get currentElapsed => elapsed.value;

  /// How long it was paused for in total.
  Duration get pausedTotal => _pausedTotal;

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

  @override
  FutureOr<dynamic> onDispose() {
    _timer?.cancel();
    _timer = null;

    elapsed.dispose();
    isRunning.dispose();

    initialize.dispose();
    start.dispose();
    pause.dispose();
    resume.dispose();
    stop.dispose();
    reset.dispose();
  }

  void _initialize() {
    final sinceStart = DateTime.now().difference(_broadcastStartTime);
    elapsed.value = sinceStart > _initialElapsed ? sinceStart : _initialElapsed;
    _logger.d(
      'Timer: starting from ${elapsed.value.inSeconds}s',
      tag: MenoLogTag.lifecycle.label,
    );
  }

  void _start() {
    if (_timer?.isActive ?? false) return;
    isRunning.value = true;
    _pausedAt = null;
    _timer = Timer.periodic(const Duration(seconds: 1), (_) => _tick());
  }

  void _pause() {
    if (!isRunning.value) return;
    _timer?.cancel();
    _timer = null;
    _pausedAt = DateTime.now();
    isRunning.value = false;
  }

  void _resume() {
    if (isRunning.value) return;
    final pausedAt = _pausedAt;
    if (pausedAt != null) {
      _pausedTotal += DateTime.now().difference(pausedAt);
    }
    _start();
  }

  void _stop() {
    _timer?.cancel();
    _timer = null;
    isRunning.value = false;
  }

  void _reset() {
    _timer?.cancel();
    _timer = null;
    isRunning.value = false;
    elapsed.value = Duration.zero;
    _pausedTotal = Duration.zero;
    _pausedAt = null;
  }

  void _tick() {
    elapsed.value =
        DateTime.now().difference(_broadcastStartTime) - _pausedTotal;
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
}
