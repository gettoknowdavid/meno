import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// The editor's header at the narrowest phone width the app supports.
///
/// The header is a title plus, at most, a "Drafts (n)" text action and a close
/// icon. This measures what the title is actually given at 320dp, because "it
/// fits" is a claim about the layout, and the layout is the thing that
/// regresses silently when an action is added to the row.
void main() {
  // The test binding draws every glyph as a full em square, which would make
  // "Go Live Now" 220 wide no matter what the real font does. The app's own
  // face is loaded so the measurement means something.
  setUpAll(() async {
    final loader = FontLoader('SFProDisplay')
      ..addFont(
        rootBundle.load('packages/ui/assets/fonts/SFProDisplay_Medium.ttf'),
      )
      ..addFont(
        rootBundle.load('packages/ui/assets/fonts/SFProDisplay_Bold.ttf'),
      );
    await loader.load();
  });

  /// Pumps the header with the two actions the page renders, at their real
  /// sizes: the drafts label with its padding, the 8dp gap, the 48dp close
  /// button and the 16dp trailing inset.
  Future<void> pumpHeader(WidgetTester tester, {required double width}) async {
    // One logical pixel per physical pixel: `setSurfaceSize` is in physical
    // pixels, so a 3x device ratio would make "320dp" a 107dp screen.
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.resetDevicePixelRatio);

    await tester.binding.setSurfaceSize(Size(width, 640));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await tester.pumpWidget(
      MaterialApp(
        theme: MTheme.dark,
        home: Builder(
          builder: (context) {
            final colors = MColorScheme.of(context);
            return MScaffold(
              appBar: MAppBar.secondary(
                title: 'Go Live Now',
                implyLeading: false,
                titleTextStyle: MTextTheme.of(context).heading3Bold,
                actions: [
                  Padding(
                    padding: const EdgeInsets.symmetric(
                      horizontal: Insets.sm,
                      vertical: Insets.md,
                    ),
                    child: MText(
                      'Drafts (3)',
                      color: colors.onBackgroundVariant,
                    ),
                  ),
                  Spaces.horizontalSmall,
                  const SizedBox(width: Insets.xxxl, height: Insets.xxxl),
                  Spaces.horizontalLarge,
                ],
              ),
              body: const SizedBox.shrink(),
            );
          },
        ),
      ),
    );
    await tester.pumpAndSettle();
  }

  testWidgets('the title fits whole at 320dp beside both header actions', (
    tester,
  ) async {
    await pumpHeader(tester, width: 320);

    final paragraph = tester.renderObject<RenderParagraph>(
      find.descendant(
        of: find.text('Go Live Now'),
        matching: find.byType(RichText),
      ),
    );

    final natural = TextPainter(
      text: paragraph.text,
      textDirection: TextDirection.ltr,
      textScaler: TextScaler.noScaling,
    )..layout();

    expect(
      paragraph.didExceedMaxLines,
      isFalse,
      reason: 'the title must not ellipsize at 320dp',
    );
    expect(paragraph.size.width, greaterThanOrEqualTo(natural.width - 0.5));
  });
}
