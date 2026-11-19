import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/settings/pages/faqs_page.dart';
import 'package:meno_ui/meno_ui.dart';

void main() {
  /// Pumps the page on its own, with the app's real theme.
  ///
  /// No locator and no router: this screen reads nothing from either, and a
  /// test that stood up the whole app to check a list would be slow for no
  /// extra coverage.
  Future<void> pumpFaqs(WidgetTester tester) async {
    await tester.pumpWidget(
      MaterialApp(theme: MTheme.light, home: const FaqsPage()),
    );
    await tester.pumpAndSettle();
  }

  Finder searchField() => find.byType(TextField).first;

  testWidgets('lists questions with the answers collapsed', (tester) async {
    await pumpFaqs(tester);

    expect(find.text('FAQs'), findsOneWidget);
    expect(find.text('What is Menō?'), findsOneWidget);
    // The answer is in the tree but not painted until the tile is opened.
    expect(
      find.textContaining('live audio platform'),
      findsNothing,
      reason: 'a screen of expanded answers is unreadable',
    );
  });

  testWidgets('tapping a question reveals its answer', (tester) async {
    await pumpFaqs(tester);

    await tester.tap(find.text('What is Menō?'));
    await tester.pumpAndSettle();

    expect(find.textContaining('live audio platform'), findsOneWidget);
  });

  testWidgets('search narrows the list to matching questions', (
    tester,
  ) async {
    await pumpFaqs(tester);

    await tester.enterText(searchField(), 'password');
    await tester.pumpAndSettle();

    expect(find.text('I forgot my password'), findsOneWidget);
    expect(find.text('What is Menō?'), findsNothing);
  });

  testWidgets('a search with no match says so instead of going blank', (
    tester,
  ) async {
    await pumpFaqs(tester);

    await tester.enterText(searchField(), 'zzzzz');
    await tester.pumpAndSettle();

    expect(find.text('No question matches that'), findsOneWidget);
    expect(find.text('What is Menō?'), findsNothing);
  });

  testWidgets('clearing the search brings every section back', (
    tester,
  ) async {
    await pumpFaqs(tester);

    await tester.enterText(searchField(), 'password');
    await tester.pumpAndSettle();
    expect(find.text('What is Menō?'), findsNothing);

    await tester.tap(find.byIcon(Icons.clear));
    await tester.pumpAndSettle();

    expect(find.text('What is Menō?'), findsOneWidget);
  });

  testWidgets('offers a way to a human when no answer fits', (tester) async {
    await pumpFaqs(tester);

    // The footer sits under every section, far past the test window.
    await tester.drag(find.byType(ListView), const Offset(0, -6000));
    await tester.pumpAndSettle();

    await tester.tap(find.text('Still need help?'));
    await tester.pumpAndSettle();

    expect(find.text('Report an app problem'), findsOneWidget);
  });
}
