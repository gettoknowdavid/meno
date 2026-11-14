import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/features/broadcast/manager/broadcast_timer_manager.dart';

import '../../_support/fake_clock.dart';
import '../../_support/stub_api.dart';

void main() {
  late FakeClock clock;
  late BroadcastTimerManager timer;

  setUp(() {
    clock = FakeClock(DateTime.utc(2026, 10, 28, 9));
    timer = BroadcastTimerManager(
      clock: clock,
      logger: MockMenoLogger(),
      initialElapsed: const Duration(minutes: 2, seconds: 7),
    );
    addTearDown(timer.onDispose);
  });

  test('the initial elapsed survives the first tick', () {
    expect(timer.currentElapsed, const Duration(minutes: 2, seconds: 7));

    timer.start();
    clock.advance(const Duration(seconds: 3));
    timer.refresh();

    expect(timer.currentElapsed, const Duration(minutes: 2, seconds: 10));
    expect(timer.formatted, '00:02:10');
    expect(timer.isRunning.value, isTrue);
  });

  test('a wall-clock correction does not move the elapsed time', () {
    timer.start();
    clock.advance(const Duration(seconds: 5));
    timer.refresh();
    expect(timer.currentElapsed, const Duration(minutes: 2, seconds: 12));

    // The device syncs its clock two hours ahead: the elapsed time is measured
    // on the monotonic reading, so nothing changes.
    clock.jumpWallClock(const Duration(hours: 2));
    timer.refresh();
    expect(timer.currentElapsed, const Duration(minutes: 2, seconds: 12));

    clock.advance(const Duration(seconds: 2));
    timer.refresh();
    expect(timer.currentElapsed, const Duration(minutes: 2, seconds: 14));
  });

  test('server reconciliation moves the clock once, and only once', () {
    timer.start();
    clock.advance(const Duration(seconds: 10));
    timer.refresh();
    expect(timer.currentElapsed, const Duration(minutes: 2, seconds: 17));

    timer.reconcile(const Duration(minutes: 5));
    expect(timer.currentElapsed, const Duration(minutes: 5));

    clock.advance(const Duration(seconds: 2));
    timer.refresh();
    expect(timer.currentElapsed, const Duration(minutes: 5, seconds: 2));

    // A second answer — a stale one, by now — is ignored.
    timer.reconcile(const Duration(minutes: 9));
    expect(timer.currentElapsed, const Duration(minutes: 5, seconds: 2));
  });

  test('pausing subtracts the paused stretch instead of forgetting it', () {
    timer.start();
    clock.advance(const Duration(seconds: 4));
    timer.pause();

    clock.advance(const Duration(seconds: 30));
    timer.refresh();
    expect(timer.currentElapsed, const Duration(minutes: 2, seconds: 11));
    expect(timer.pausedTotal, const Duration(seconds: 30));

    timer.resume();
    clock.advance(const Duration(seconds: 1));
    timer.refresh();
    expect(timer.currentElapsed, const Duration(minutes: 2, seconds: 12));
  });
}
