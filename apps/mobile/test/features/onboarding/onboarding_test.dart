import 'package:flutter_secure_storage/flutter_secure_storage.dart';
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
    // Secure storage starts empty and in memory, so the startup gate can
    // restore a session without a platform channel.
    FlutterSecureStorage.setMockInitialValues({});
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
      StorageKeys.onboardingCompleted: true,
    });
    configureDependencies(_config);

    await tester.pumpWidget(const MenoApp());
    await pumpUntilFound(tester, find.text('Log in'));

    expect(find.text('Get started'), findsNothing);
    expect(find.text('Log in'), findsWidgets);
  });

  testWidgets('leaving the intro opens registration, unrecorded', (
    tester,
  ) async {
    SharedPreferences.setMockInitialValues({});
    // Secure storage starts empty and in memory, so the startup gate can
    // restore a session without a platform channel.
    FlutterSecureStorage.setMockInitialValues({});
    configureDependencies(_config);

    await tester.pumpWidget(const MenoApp());
    await pumpUntilFound(tester, find.text('Get started'));

    // The actions sit below the fold of the default test window.
    await tester.ensureVisible(find.text('Get started'));
    await tester.pump();
    await tester.tap(find.text('Get started'));
    await tester.pumpAndSettle();

    expect(find.widgetWithText(AppBar, 'New Account'), findsOneWidget);
    expect(find.text('Get started'), findsNothing);
    // Opening a form is not the same thing as finishing the intro: only an
    // account does that, so a restart still shows the intro.
    expect(di<OnboardingService>().isOnboarded, isFalse);

    await unmountTree(tester);
  });

  testWidgets('the Login action opens the login route, unrecorded', (
    tester,
  ) async {
    SharedPreferences.setMockInitialValues({});
    // Secure storage starts empty and in memory, so the startup gate can
    // restore a session without a platform channel.
    FlutterSecureStorage.setMockInitialValues({});
    configureDependencies(_config);

    await tester.pumpWidget(const MenoApp());
    await pumpUntilFound(tester, find.text('Login'));

    // The actions sit below the fold of the default test window.
    await tester.ensureVisible(find.text('Login'));
    await tester.pump();
    await tester.tap(find.text('Login'));
    await tester.pumpAndSettle();

    expect(find.text('Get started'), findsNothing);
    expect(find.widgetWithText(AppBar, 'Log in'), findsOneWidget);
    expect(di<OnboardingService>().isOnboarded, isFalse);

    await unmountTree(tester);
  });

  testWidgets('restarting without signing in brings the intro back', (
    tester,
  ) async {
    SharedPreferences.setMockInitialValues({});
    // Secure storage starts empty and in memory, so the startup gate can
    // restore a session without a platform channel.
    FlutterSecureStorage.setMockInitialValues({});
    configureDependencies(_config);

    await tester.pumpWidget(const MenoApp());
    await pumpUntilFound(tester, find.text('Login'));
    await tester.ensureVisible(find.text('Login'));
    await tester.pump();
    await tester.tap(find.text('Login'));
    await tester.pumpAndSettle();
    expect(find.widgetWithText(AppBar, 'Log in'), findsOneWidget);

    // The restart: nothing was signed in, so nothing about the install changed.
    await unmountTree(tester);
    await di.reset();
    // Disposing the auth manager schedules a timer of its own; let it fire so
    // the test does not end with one pending.
    await tester.pump(const Duration(milliseconds: 200));
    configureDependencies(_config);

    await tester.pumpWidget(const MenoApp());
    await pumpUntilFound(tester, find.text('Get started'));

    expect(find.text('Get started'), findsOneWidget);
    expect(find.widgetWithText(AppBar, 'Log in'), findsNothing);

    await unmountTree(tester);
  });
}
