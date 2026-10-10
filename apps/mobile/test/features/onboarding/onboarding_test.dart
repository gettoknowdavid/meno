import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/app.dart';
import 'package:meno_mobile/features/onboarding/services/onboarding_service.dart';
import 'package:meno_mobile/locator.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../../_support/pump_until_found.dart';

const _config = Config(
  env: Environment.dev,
  menoApiUrl: 'https://dev.api',
  webSocketUrl: 'wss://dev.socket',
  menoLiveKitUrl: 'https://dev.livekit',
);

void main() {
  setUp(di.reset);
  tearDown(di.reset);

  testWidgets('a fresh install opens the intro carousel', (tester) async {
    SharedPreferences.setMockInitialValues({});
    configureDependencies(_config);

    await tester.pumpWidget(const MenoApp());
    await pumpUntilFound(tester, find.text('Get started'));

    expect(find.text('Broadcast'), findsOneWidget);
    expect(find.text('Get started'), findsOneWidget);
    expect(find.text('Login'), findsOneWidget);

    await unmountTree(tester);
  });

  testWidgets('an install that finished the intro skips it', (tester) async {
    SharedPreferences.setMockInitialValues({
      OnboardingService.onboardingKey: true,
    });
    configureDependencies(_config);

    await tester.pumpWidget(const MenoApp());
    await tester.pumpAndSettle();

    expect(find.text('Get started'), findsNothing);
    expect(find.text('Login'), findsWidgets);
  });

  testWidgets('leaving the intro records it and opens registration', (
    tester,
  ) async {
    SharedPreferences.setMockInitialValues({});
    configureDependencies(_config);

    await tester.pumpWidget(const MenoApp());
    await pumpUntilFound(tester, find.text('Get started'));

    // The actions sit below the fold of the default test window.
    await tester.ensureVisible(find.text('Get started'));
    await tester.pump();
    await tester.tap(find.text('Get started'));
    await tester.pumpAndSettle();

    expect(find.text('Register'), findsWidgets);
    expect(find.text('Get started'), findsNothing);
    expect(
      di<SharedPreferences>().getBool(OnboardingService.onboardingKey),
      isTrue,
    );

    await unmountTree(tester);
  });

  testWidgets('the Login action records the intro and opens the login route', (
    tester,
  ) async {
    SharedPreferences.setMockInitialValues({});
    configureDependencies(_config);

    await tester.pumpWidget(const MenoApp());
    await pumpUntilFound(tester, find.text('Login'));

    // The actions sit below the fold of the default test window.
    await tester.ensureVisible(find.text('Login'));
    await tester.pump();
    await tester.tap(find.text('Login'));
    await tester.pumpAndSettle();

    expect(find.text('Get started'), findsNothing);
    expect(find.widgetWithText(AppBar, 'Login'), findsOneWidget);
    expect(
      di<SharedPreferences>().getBool(OnboardingService.onboardingKey),
      isTrue,
    );

    await unmountTree(tester);
  });
}
