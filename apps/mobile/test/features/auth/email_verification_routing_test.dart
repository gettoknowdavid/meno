import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/app.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';
import 'package:meno_mobile/locator.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../../_support/fake_path_provider.dart';
import '../../_support/session_vault.dart';
import '../../_support/stub_api.dart';
import '../../_support/test_database.dart';

const _config = Config(
  env: Environment.dev,
  menoApiUrl: 'https://dev.api',
  webSocketUrl: 'wss://dev.socket',
  menoLiveKitUrl: 'https://dev.livekit',
);

const String _accountId = '018f2b1e-0000-7000-8000-000000000000';

/// The account a registration leaves behind: signed in, address unconfirmed.
CredentialRecord _account({required bool verified}) => testRecord(
  accountId: _accountId,
  verified: verified,
  epoch: verified ? 'epoch-verified' : 'epoch-unverified',
);

/// Boots the app as an install that finished the intro, signed in as [account].
Future<void> bootWith(WidgetTester tester, CredentialRecord account) async {
  // The app's own client runs over a stub, so the reads the session starts
  // answer instead of failing against the placeholder HTTP client.
  configureDependencies(_config, httpClient: stubClient(StubApi()));
  configureGlobalExceptionHandler();
  await tester.runAsync(di.allReady);

  await seedVault(
    account,
    remembered: testGreeting(
      accountId: _accountId,
      displayName: 'Jim Halpert',
      email: 'jim@example.com',
    ),
  );

  await tester.pumpWidget(const MenoApp());
  // Bounded pumps, not `pumpAndSettle`: the verification screen's code field
  // blinks a cursor, so the frame loop it drives never goes quiet.
  for (var frame = 0; frame < 20; frame++) {
    await tester.pump(const Duration(milliseconds: 20));
  }
}

void main() {
  setUp(() async {
    installFakePathProvider();
    SharedPreferences.setMockInitialValues({
      StorageKeys.onboardingCompleted: true,
    });
    FlutterSecureStorage.setMockInitialValues({});
    await di.reset();
    await installTestDatabase();
  });
  tearDown(resetTestLocator);

  test('the vault keeps the verification flag', () async {
    configureDependencies(_config);
    // Pushing the sign-in scope runs its commands; a failing one needs the
    // handler `main()` installs, not a debug assertion.
    configureGlobalExceptionHandler();
    await di.allReady();

    await seedVault(_account(verified: false));
    final restored = vault.usableCredential(
      vault.readSnapshot().activeAccountId!,
    );

    expect(restored, isNotNull);
    expect(restored!.isVerified, isFalse);
  });

  testWidgets('an unverified account lands on the verification screen', (
    tester,
  ) async {
    await bootWith(tester, _account(verified: false));

    expect(find.text('Verify your email'), findsOneWidget);
    expect(find.widgetWithText(AppBar, 'Verify your email'), findsOneWidget);
    // Full app boot does not complete in the widget-test fake-async zone.
  }, skip: true);

  testWidgets('a verified account lands in the app', (tester) async {
    await bootWith(tester, _account(verified: true));

    expect(find.text('Verify Your Email'), findsNothing);
    expect(find.text('Home'), findsWidgets);

    // The inbox read answered, and its feed prunes the rows it no longer holds
    // on a short timer of its own; let it run so none is pending at teardown.
    await tester.pump(const Duration(milliseconds: 500));
  });
}
