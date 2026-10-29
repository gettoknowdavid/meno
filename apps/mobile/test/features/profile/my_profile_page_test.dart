import 'dart:convert';

import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/app.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';
import 'package:meno_mobile/features/profile/profile.dart';
import 'package:meno_mobile/locator.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../../_support/fake_path_provider.dart';
import '../../_support/pump_until_found.dart';
import '../../_support/session_vault.dart';

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

/// Where this account's own cache entry lives: viewer and subject are the same.
const String _profileKey = 'profile.v2/$_accountId/$_accountId';

/// The record `GET /users/me` would have answered with, as the cache stores it.
///
/// The counters are deliberately not zeros: a screen that reads them wrongly
/// still shows a number, and this is what tells the labels apart.
final String _cachedProfile = jsonEncode({
  'id': _accountId,
  'full_name': 'Ada Lovelace',
  'email': 'ada@example.com',
  'email_verified': true,
  'bio': 'Writes the first program, most days.',
  'followers': 7,
  'following': 5,
  'broadcasts': 11,
});

void main() {
  setUp(() async {
    installFakePathProvider();
    await di.reset();
    FlutterSecureStorage.setMockInitialValues({});
  });
  tearDown(di.reset);

  /// Boots the app signed in and past onboarding, so the router lands in the
  /// shell with an account whose id is `1`.
  Future<void> boot(WidgetTester tester) async {
    configureDependencies(_config);
    // `main()` does this after the locator is up. In a widget test every
    // request is refused, so the profile read has to have somewhere to land.
    configureGlobalExceptionHandler();

    await di.allReady();
    await seedVault(
      _account,
      remembered: testGreeting(
        accountId: _accountId,
        displayName: 'Ada Lovelace',
        email: 'ada@example.com',
      ),
    );

    await tester.pumpWidget(const MenoApp());
    await tester.pumpAndSettle();
  }

  /// Opens the profile tab.
  Future<void> openProfile(WidgetTester tester) async {
    await tester.tap(find.text('Profile'));
    await tester.pumpAndSettle();
  }

  /// Sizes the window, and generously.
  ///
  /// The font tests render under draws every glyph a full em wide, so the
  /// header's fixed columns and the upgrade row — which fit a real phone —
  /// only fit a window this wide here. Nothing about the screen changes with
  /// the window: the assertions are about what it shows.
  void useWindow(WidgetTester tester, {required Size size}) {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = size;
    addTearDown(tester.view.reset);
  }

  testWidgets('what is on disk paints the screen before the network answers', (
    tester,
  ) async {
    useWindow(tester, size: const Size(700, 900));

    SharedPreferences.setMockInitialValues({
      StorageKeys.onboardingCompleted: true,
      _profileKey: _cachedProfile,
    });
    await boot(tester);
    await openProfile(tester);

    expect(find.byType(MyProfilePage), findsOneWidget);

    // The header's layout, not just its name: the counters and the buttons
    // that sit under it.
    expect(find.text('Broadcasts'), findsOneWidget);
    expect(find.text('Subscribers'), findsOneWidget);
    expect(find.text('Subscriptions'), findsOneWidget);
    expect(find.text('FREE ACCOUNT'), findsOneWidget);
    expect(find.text('Edit profile'), findsOneWidget);
    expect(find.text('Share profile'), findsOneWidget);
    expect(find.text('Writes the first program, most days.'), findsOneWidget);

    // The tab bar the header pins: three tabs, all feeds that belong to other
    // features.
    expect(find.text('Recent broadcasts'), findsOneWidget);
    expect(find.text('All broadcasts'), findsOneWidget);
    expect(find.text('Favorites'), findsOneWidget);

    await unmountTree(tester);
  });

  testWidgets('a profile that cannot be read offers a retry', (tester) async {
    useWindow(tester, size: const Size(700, 900));

    // Nothing on disk and nothing from the API: the binding refuses every
    // request, so this is the "first open, no signal" case.
    SharedPreferences.setMockInitialValues({
      StorageKeys.onboardingCompleted: true,
    });
    await boot(tester);
    await openProfile(tester);

    expect(
      find.text(
        "We couldn't load your profile. "
        'Please check your connection, then try again.',
      ),
      findsOneWidget,
    );
    expect(find.text('Retry'), findsOneWidget);

    await unmountTree(tester);
  });
}
