import 'package:material_ui/material_ui.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_ui/meno_ui.dart';

void main() {
  group('MTheme', () {
    for (final brightness in Brightness.values) {
      test('registers every extension for $brightness', () {
        final theme = MTheme.theme(brightness);
        final extensions = theme.extensions.values;

        expect(extensions.whereType<MColorScheme>(), hasLength(1));
        expect(extensions.whereType<MTextTheme>(), hasLength(1));
        expect(extensions.whereType<MButtonStyles>(), hasLength(1));
        expect(extensions.whereType<MCardStyles>(), hasLength(1));
        expect(extensions.whereType<MGlobalStyles>(), hasLength(1));
        expect(extensions.whereType<MModalStyles>(), hasLength(1));
        expect(extensions.whereType<MNavigationStyles>(), hasLength(1));
        expect(extensions.whereType<MOtpFieldStyles>(), hasLength(1));
        expect(extensions.whereType<MTabBarTheme>(), hasLength(1));
        expect(extensions.whereType<MTextFieldStyle>(), hasLength(1));

        expect(theme.colorScheme.brightness, brightness);
        expect(
          theme.scaffoldBackgroundColor,
          theme.extension<MColorScheme>()!.background,
        );
      });
    }
  });

  group('MColorScheme tokens', () {
    test('resolve brand colors per brightness', () {
      final light = MColorScheme.$default(Brightness.light);
      final dark = MColorScheme.$default(Brightness.dark);

      expect(light.primary, MColor.primary300);
      expect(dark.primary, MColor.primary75);
      expect(light.background, MColor.white);
      expect(dark.background, MColor.primary700);
      expect(light.onBackground, MColor.black);
      expect(dark.onBackground, MColor.white);
    });
  });

  group('MTextTheme tokens', () {
    test('build the type scale from the color scheme', () {
      final light = MTextTheme.$default(
        MColorScheme.$default(Brightness.light),
      );

      expect(light.bodyRegular.fontSize, 16);
      expect(light.bodyRegular.fontWeight, FontWeight.w500);
      expect(light.bodyMedium.fontWeight, FontWeight.w600);
      expect(light.bodyBold.fontWeight, FontWeight.w700);
      expect(light.captionRegular.fontSize, 14);
      expect(light.countDown.fontSize, 72);
      expect(light.bodyRegular.color, MColor.black);
    });
  });

  group('spacing tokens', () {
    test('Spaces mirror the Insets scale', () {
      expect(Spaces.verticalMicro.height, Insets.xs);
      expect(Spaces.verticalSmall.height, Insets.sm);
      expect(Spaces.verticalMedium.height, Insets.md);
      expect(Spaces.verticalLarge.height, Insets.lg);
      expect(Spaces.verticalXLarge.height, Insets.xl);
      expect(Spaces.verticalXXLarge.height, Insets.xxl);
      expect(Spaces.verticalXXXLarge.height, Insets.xxxl);
      expect(Spaces.horizontalMicro.width, Insets.xs);
      expect(Spaces.horizontalXXXLarge.width, Insets.xxxl);
    });
  });

  group('of(context)', () {
    testWidgets('find every extension on the enclosing theme', (tester) async {
      late BuildContext context;
      await tester.pumpWidget(
        MaterialApp(
          theme: MTheme.light,
          home: Builder(
            builder: (ctx) {
              context = ctx;
              return const SizedBox();
            },
          ),
        ),
      );

      expect(MColorScheme.of(context).brightness, Brightness.light);
      expect(MTextTheme.of(context).bodyRegular.fontSize, 16);
      expect(MButtonStyles.of(context).primary, isNotNull);
      expect(MCardStyles.of(context).titleStyle, isNotNull);
      expect(MGlobalStyles.of(context).dividerTheme, isNotNull);
      expect(MModalStyles.of(context).bottomSheetTheme, isNotNull);
      expect(MNavigationStyles.of(context).appBarTheme, isNotNull);
      expect(MOtpFieldStyles.of(context).border, isNotNull);
      expect(MTabBarTheme.of(context).height, 32);
      expect(MTextFieldStyle.of(context).border, isNotNull);
    });
  });

  group('TabBarStyle.lerp', () {
    test('interpolates each field independently', () {
      final black = WidgetStateProperty.all(Colors.black);
      final white = WidgetStateProperty.all(Colors.white);
      final a = TabBarStyle(
        labelColor: black,
        backgroundColor: black,
        dividerColor: Colors.black,
        indicator: const BoxDecoration(),
        dividerHeight: 0,
        iconTheme: WidgetStateProperty.all(const IconThemeData(size: 10)),
      );
      final b = a.copyWith(
        backgroundColor: white,
        dividerColor: Colors.white,
        dividerHeight: 2,
        iconTheme: WidgetStateProperty.all(const IconThemeData(size: 20)),
      );

      final mid = TabBarStyle.lerp(a, b, 0.5)!;

      // Regression: `backgroundColor` must lerp from its own field, not
      // `labelColor`.
      const midway = Color(0xFF808080);

      expect(mid.labelColor.resolve({}), Colors.black);
      expect(mid.backgroundColor.resolve({})!.toARGB32(), midway.toARGB32());
      expect(mid.dividerColor.toARGB32(), midway.toARGB32());
      expect(mid.dividerHeight, 1);
      expect(mid.iconTheme.resolve({})!.size, 15);
    });

    test('returns the first style when both are identical', () {
      final style = TabBarStyle(
        labelColor: WidgetStateProperty.all(Colors.black),
        backgroundColor: WidgetStateProperty.all(Colors.black),
        dividerColor: Colors.black,
        indicator: const BoxDecoration(),
        dividerHeight: 0,
        iconTheme: WidgetStateProperty.all(const IconThemeData()),
      );
      expect(identical(TabBarStyle.lerp(style, style, 0.5), style), isTrue);
    });
  });
}
