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

/// Swaps in an empty tree so timers owned by the app's widgets are cancelled
/// before the framework checks for pending timers at the end of a test.
Future<void> unmountTree(WidgetTester tester) async {
  await tester.pumpWidget(const SizedBox());
  await tester.pump();
}
