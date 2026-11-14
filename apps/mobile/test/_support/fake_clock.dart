import 'package:meno_mobile/_core/time/clock.dart';

/// A clock that only moves when a test moves it.
final class FakeClock implements Clock {
  new(DateTime start) : _now = start.toUtc();

  DateTime _now;
  Duration _monotonic = Duration.zero;

  @override
  DateTime nowUtc() => _now;

  @override
  Duration monotonic() => _monotonic;

  /// Moves both readings forward by [by].
  void advance(Duration by) {
    _now = _now.add(by);
    _monotonic += by;
  }

  /// Jumps the wall clock by [by] and leaves the monotonic reading alone.
  ///
  /// What a device does when it syncs or corrects its time: [nowUtc] changes,
  /// [monotonic] does not — which is exactly the case a broadcast clock must
  /// ignore.
  void jumpWallClock(Duration by) => _now = _now.add(by);

  /// Jumps to [at], leaving the monotonic reading alone.
  void set(DateTime at) => _now = at.toUtc();
}
