import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

/// Pumps bounded frames until [finder] matches, giving up after [attempts].
///
/// `pumpAndSettle` cannot be used on screens that run a periodic timer — the
/// intro carousel advances itself every five seconds — so tests that land on
/// one pump a fixed number of frames instead.
Future<void> pumpUntilFound(
  WidgetTester tester,
  Finder finder, {
  Duration step = const Duration(milliseconds: 20),
  int attempts = 25,
}) async {
  for (var attempt = 0; attempt < attempts; attempt++) {
    if (finder.evaluate().isNotEmpty) return;
    await tester.pump(step);
  }
}

/// Turns the event loop until [done] holds, giving up after [attempts].
///
/// For work that is asynchronous but not tied to a widget — a scope pushed in
/// reaction to a stored credential, say — where `pumpEventQueue`'s fixed number
/// of turns is a guess at how many awaits are involved. [step] is how long each
/// turn waits, which has to be non-zero when the work under test waits for a
/// real duration of its own.
Future<void> pumpUntil(
  bool Function() done, {
  Duration step = Duration.zero,
  int attempts = 100,
}) async {
  for (var attempt = 0; attempt < attempts; attempt++) {
    if (done()) return;
    await Future<void>.delayed(step);
  }
}

/// Swaps in an empty tree so timers owned by the app's widgets are cancelled
/// before the framework checks for pending timers at the end of a test.
Future<void> unmountTree(WidgetTester tester) async {
  await tester.pumpWidget(const SizedBox());
  await tester.pump();
}
