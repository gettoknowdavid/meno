import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/app.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';
import 'package:meno_mobile/features/auth/services/session_store.dart';
import 'package:meno_mobile/features/profile/profile.dart';
import 'package:meno_mobile/locator.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '_support/pump_until_found.dart';

const _config = Config(
  env: Environment.dev,
  menoApiUrl: 'https://dev.api',
  webSocketUrl: 'wss://dev.socket',
  menoLiveKitUrl: 'https://dev.livekit',
);

/// The account the sign-in flow would have left in the store: signed in, with
/// the address already confirmed.
final UserCredential _account = AuthResponseDto.fromJson({
  'access_token': 'access-token',
  'refresh_token': 'refresh-token',
  'expires_in': 3600,
  'user': {
    'id': '1',
    'full_name': 'Ada Lovelace',
    'email': 'ada@example.com',
    'verified': true,
  },
}).toDomain;

/// Boots the app signed in and past onboarding, so the router lands inside the
/// root shell.
Future<void> _bootIntoShell(WidgetTester tester) async {
  configureDependencies(_config);
  // `main()` does this after the locator is up. The profile screen starts a
  // read the moment it is opened, and in a widget test every request is
  // refused, so that failure reaches the global handler.
  configureGlobalExceptionHandler();
  await di.allReady();

  // Nothing else can reach the shell: the guard sends an unfinished intro to
  // the carousel and anybody signed out to the sign-in screen.
  await di<SessionStore>().saveActive(_account);

  await tester.pumpWidget(const MenoApp());
  await tester.pumpAndSettle();
}

/// Pins the logical window size for the duration of a test.
void _useWindow(WidgetTester tester, Size size) {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = size;
  addTearDown(tester.view.reset);
}

void main() {
  setUp(() async {
    // Onboarding already completed: the guard sends a signed-in user into the
    // shell instead of back to the intro.
    SharedPreferences.setMockInitialValues({
      StorageKeys.onboardingCompleted: true,
    });
    // Secure storage starts empty and in memory: the tests write the accounts
    // they need through the app's own store.
    FlutterSecureStorage.setMockInitialValues({});
    await di.reset();
  });
  tearDown(di.reset);

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

    await tester.tap(find.text('Profile'));
    await tester.pumpAndSettle();

    expect(find.byType(MyProfilePage), findsOneWidget);

    // The read is refused in a widget test, and the toast that says so sits
    // over the tab bar until it dismisses itself.
    await tester.pumpAndSettle(const Duration(seconds: 4));

    await tester.tap(find.text('Discover'));
    await tester.pumpAndSettle();

    expect(find.byType(MyProfilePage), findsNothing);
    expect(find.text('Discover'), findsWidgets);

    await unmountTree(tester);
  });

  testWidgets('tapping the active tab resets that branch', (tester) async {
    _useWindow(tester, const Size(390, 844));
    await _bootIntoShell(tester);

    await tester.tap(find.text('Profile'));
    await tester.pumpAndSettle();
    expect(find.byType(MyProfilePage), findsOneWidget);

    // The read is refused in a widget test, and the toast that says so sits
    // over the tab bar until it dismisses itself.
    await tester.pumpAndSettle(const Duration(seconds: 4));

    // Tapping the already-selected tab must not throw and must stay put.
    await tester.tap(find.text('Profile').first);
    await tester.pumpAndSettle();
    expect(find.byType(MyProfilePage), findsOneWidget);

    await unmountTree(tester);
  });
}
