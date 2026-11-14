import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// Pumps a themed app and returns a context below its `Overlay`.
Future<BuildContext> _pumpApp(WidgetTester tester) async {
  late BuildContext context;
  await tester.pumpWidget(
    MaterialApp(
      theme: MTheme.light,
      home: Scaffold(
        body: Builder(
          builder: (ctx) {
            context = ctx;
            return const SizedBox();
          },
        ),
      ),
    ),
  );
  return context;
}

/// The `Align` that anchors the toast showing [message].
Alignment _alignmentOf(WidgetTester tester, String message) {
  final toast = find.ancestor(
    of: find.text(message),
    matching: find.byType(MToast),
  );
  final align = find.ancestor(of: toast, matching: find.byType(Align)).first;
  return tester.widget<Align>(align).alignment as Alignment;
}

void main() {
  group('showToast', () {
    testWidgets('renders a toast and auto-dismisses it', (tester) async {
      final context = await _pumpApp(tester);

      context.showToast('Saved', duration: const Duration(seconds: 1));
      await tester.pump();
      await tester.pumpAndSettle();

      expect(find.byType(MToast), findsOneWidget);
      expect(find.text('Saved'), findsOneWidget);

      await tester.pump(const Duration(seconds: 1));
      await tester.pumpAndSettle();

      expect(find.byType(MToast), findsNothing);
    });

    testWidgets('anchors each toast to its position', (tester) async {
      final context = await _pumpApp(tester);

      context
        ..showToast('Top left', position: .topLeft, duration: Duration.zero)
        ..showToast(
          'Bottom right',
          position: .bottomRight,
          duration: Duration.zero,
        );
      await tester.pump();
      await tester.pumpAndSettle();

      expect(_alignmentOf(tester, 'Top left'), Alignment.topLeft);
      expect(_alignmentOf(tester, 'Bottom right'), Alignment.bottomRight);

      context.clearToasts();
      await tester.pumpAndSettle();
    });

    testWidgets('scales a centred toast instead of sliding it', (tester) async {
      final context = await _pumpApp(tester);

      context.showToast('Centred', position: .center, duration: Duration.zero);
      await tester.pump();
      await tester.pumpAndSettle();

      expect(
        find.ancestor(
          of: find.byType(MToast),
          matching: find.byType(SlideTransition),
        ),
        findsNothing,
      );
      expect(
        find.ancestor(
          of: find.byType(MToast),
          matching: find.byType(ScaleTransition),
        ),
        findsWidgets,
      );

      context.clearToasts();
      await tester.pumpAndSettle();
    });

    testWidgets('queues toasts past maxVisible and promotes them', (
      tester,
    ) async {
      final context = await _pumpApp(tester);

      final ids = [
        for (var i = 0; i < 4; i++)
          context.showToast('Toast $i', duration: Duration.zero),
      ];
      await tester.pump();
      await tester.pumpAndSettle();

      expect(find.byType(MToast), findsNWidgets(3));
      expect(find.text('Toast 3'), findsNothing);

      context.dismissToast(ids.first);
      await tester.pumpAndSettle();

      expect(find.byType(MToast), findsNWidgets(3));
      expect(find.text('Toast 3'), findsOneWidget);

      context.clearToasts();
      await tester.pumpAndSettle();
      expect(find.byType(MToast), findsNothing);
    });

    testWidgets('dismisses through the close button', (tester) async {
      final context = await _pumpApp(tester);

      context.showToast('Closable', duration: Duration.zero);
      await tester.pump();
      await tester.pumpAndSettle();
      expect(find.byType(MToast), findsOneWidget);

      await tester.tap(find.byIcon(MIcons.x_close));
      await tester.pumpAndSettle();

      expect(find.byType(MToast), findsNothing);
    });

    testWidgets('showErrorToast uses the error variant', (tester) async {
      final context = await _pumpApp(tester);

      context.showErrorToast('Failed', duration: Duration.zero);
      await tester.pump();
      await tester.pumpAndSettle();

      expect(
        tester.widget<MToast>(find.byType(MToast)).variant,
        MToastVariant.error,
      );
      expect(find.byIcon(MIcons.x_circle), findsOneWidget);

      context.clearToasts();
      await tester.pumpAndSettle();
    });
  });

  group('showModal', () {
    testWidgets('opens a themed sheet and resolves to its result', (
      tester,
    ) async {
      final context = await _pumpApp(tester);

      final result = context.showModal<String>(
        Builder(
          builder: (sheetContext) => TextButton(
            onPressed: () => Navigator.of(sheetContext).pop('picked'),
            child: const Text('Pick'),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('Pick'), findsOneWidget);

      await tester.tap(find.text('Pick'));
      await tester.pumpAndSettle();

      expect(await result, 'picked');
    });
  });
}
