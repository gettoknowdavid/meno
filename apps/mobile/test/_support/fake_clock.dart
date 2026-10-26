import 'package:meno_mobile/_core/time/clock.dart';

/// A clock that only moves when a test moves it.
final class FakeClock implements Clock {
  new(DateTime start) : _now = start.toUtc();

  DateTime _now;

  @override
  DateTime nowUtc() => _now;

  /// Moves the clock forward by [by].
  void advance(Duration by) => _now = _now.add(by);

  /// Jumps to [at].
  void set(DateTime at) => _now = at.toUtc();
}
