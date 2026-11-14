import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

Future<void> _pumpPage(WidgetTester tester, Widget page) {
  return tester.pumpWidget(MaterialApp(theme: MTheme.light, home: page));
}

void main() {
  group('MLoadingPage', () {
    testWidgets('fills the screen with the loading indicator', (tester) async {
      await _pumpPage(tester, const MLoadingPage());

      expect(find.byType(MLoadingPage), findsOneWidget);
      expect(find.byType(MLoadingIndicator), findsOneWidget);
      expect(find.byType(MScaffold), findsOneWidget);
      expect(find.byType(Scaffold), findsOneWidget);
    });

    testWidgets('shows its message when one is given', (tester) async {
      await _pumpPage(
        tester,
        const MLoadingPage(message: 'Starting your live session'),
      );

      expect(find.text('Starting your live session'), findsOneWidget);
    });

    testWidgets('renders no copy when no message is given', (tester) async {
      await _pumpPage(tester, const MLoadingPage());

      expect(find.byType(MText), findsNothing);
    });
  });
}
