import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/app.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';
import 'package:meno_mobile/features/discover/discover.dart';
import 'package:meno_mobile/features/profile/profile.dart';
import 'package:meno_mobile/locator.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '_support/fake_path_provider.dart';
import '_support/pump_until_found.dart';
import '_support/session_vault.dart';
import '_support/stub_api.dart';
import '_support/test_database.dart';

const _config = Config(
  env: Environment.dev,
  menoApiUrl: 'https://dev.api',
  webSocketUrl: 'wss://dev.socket',
  menoLiveKitUrl: 'https://dev.livekit',
);

/// The account the sign-in flow would have left in the vault: signed in, with
/// the address already confirmed.
const String _accountId = '018f2b1e-0000-7000-8000-000000000001';

final CredentialRecord _account = testRecord(accountId: _accountId);

/// Boots the app signed in and past onboarding, so the router lands inside the
/// root shell.
Future<void> _bootIntoShell(WidgetTester tester) async {
  // The app's own client runs over a stub, so the reads the shell starts answer
  // instead of failing against the platform's placeholder HTTP client.
  configureDependencies(_config, httpClient: stubClient(StubApi()));
  // `main()` does this after the locator is up. The profile screen starts a
  // read the moment it is opened, and in a widget test every request is
  // refused, so that failure reaches the global handler.
  configureGlobalExceptionHandler();
  await di.allReady();

  // Nothing else can reach the shell: the guard sends an unfinished intro to
  // the carousel and anybody signed out to the sign-in screen.
  await seedVault(
    _account,
    remembered: testGreeting(
      accountId: _accountId,
      displayName: 'Ada Lovelace',
      email: 'ada@example.com',
    ),
  );

  await tester.pumpWidget(const MenoApp());
  await _settle(tester);
}

/// Pumps bounded frames.
///
/// `pumpAndSettle` never returns on a screen with a repeating animation — the
/// profile's skeletons shimmer while a read is in flight — so the frames are
/// pumped a fixed number of times instead.
Future<void> _settle(WidgetTester tester) async {
  for (var frame = 0; frame < 25; frame++) {
    await tester.pump(const Duration(milliseconds: 20));
  }
}

/// Lets any error toast from the failed reads dismiss itself.
///
/// A toast is an overlay over the bottom of the screen: it takes the tap that
/// was meant for the tab under it.
Future<void> _dismissToasts(WidgetTester tester) async {
  await tester.pump(const Duration(seconds: 4));
  await _settle(tester);
}

/// Pins the logical window size for the duration of a test.
void _useWindow(WidgetTester tester, Size size) {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = size;
  addTearDown(tester.view.reset);
}

void main() {
  setUp(() async {
    installFakePathProvider();
    // Onboarding already completed: the guard sends a signed-in user into the
    // shell instead of back to the intro.
    SharedPreferences.setMockInitialValues({
      StorageKeys.onboardingCompleted: true,
    });
    // Secure storage starts empty and in memory: the tests write the accounts
    // they need through the app's own store.
    FlutterSecureStorage.setMockInitialValues({});
    await di.reset();
    // The app opens a store as it starts; a widget test's fake-async zone never
    // completes that, so it gets an in-memory one instead.
    await installTestDatabase();
  });
  tearDown(resetTestLocator);

  testWidgets('uses the bottom bar on a phone-sized window', (tester) async {
    _useWindow(tester, const Size(390, 844));
    await _bootIntoShell(tester);

    expect(find.byType(MBottomNavigationBar), findsOneWidget);
    expect(find.byType(MSideNavRail), findsNothing);
    expect(find.text('Home'), findsWidgets);
  });

  testWidgets('uses the side rail on a wide window', (tester) async {
    _useWindow(tester, const Size(1280, 900));
    await _bootIntoShell(tester);

    expect(find.byType(MSideNavRail), findsOneWidget);
    expect(find.byType(MBottomNavigationBar), findsNothing);
  });

  testWidgets('tapping a tab switches the router branch', (tester) async {
    _useWindow(tester, const Size(390, 844));
    await _bootIntoShell(tester);

    expect(find.byType(MyProfilePage), findsNothing);

    await _dismissToasts(tester);
    await tester.tap(find.text('Profile'));
    await _settle(tester);

    expect(find.byType(MyProfilePage), findsOneWidget);

    // The read is refused in a widget test, and the toast that says so sits
    // over the tab bar until it dismisses itself.
    await _dismissToasts(tester);
    await tester.tap(find.text('Discover'));
    await _settle(tester);

    expect(find.byType(MyProfilePage), findsNothing);
    expect(find.byType(AllBroadcastsWidget), findsOneWidget);

    await unmountTree(tester);
  });

  testWidgets('tapping the active tab resets that branch', (tester) async {
    _useWindow(tester, const Size(390, 844));
    await _bootIntoShell(tester);

    await _dismissToasts(tester);
    await tester.tap(find.text('Profile'));
    await _settle(tester);
    expect(find.byType(MyProfilePage), findsOneWidget);

    // The read is refused in a widget test, and the toast that says so sits
    // over the tab bar until it dismisses itself.
    await _dismissToasts(tester);

    // Tapping the already-selected tab must not throw and must stay put.
    await tester.tap(find.text('Profile').first);
    await _settle(tester);
    expect(find.byType(MyProfilePage), findsOneWidget);

    await unmountTree(tester);
  });
}
