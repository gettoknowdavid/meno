import 'dart:convert';

import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
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

const String _accountId = '018f2b1e-0000-7000-8000-000000000002';

final CredentialRecord _account = testRecord(accountId: _accountId);

/// Where this account's own cache entry lives.
const String _profileKey = 'profile.v2/$_accountId/$_accountId';

/// The record the account's own cache holds, as the API would have answered it.
final String _cachedProfile = jsonEncode({
  'id': _accountId,
  'full_name': 'Ada Lovelace',
  'email': 'ada@example.com',
  'email_verified': true,
  'bio': 'Writes the first program, most days.',
});

void main() {
  setUp(() async {
    installFakePathProvider();
    await di.reset();
    FlutterSecureStorage.setMockInitialValues({});
  });
  tearDown(di.reset);

  /// Boots the app signed in, past onboarding, with the record on disk.
  Future<void> boot(WidgetTester tester) async {
    SharedPreferences.setMockInitialValues({
      StorageKeys.onboardingCompleted: true,
      _profileKey: _cachedProfile,
    });

    configureDependencies(_config);
    // `main()` does this after the locator is up; the account's own read has
    // nowhere to land in a widget test otherwise.
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

  testWidgets('the chrome reads the profile manager, never the credential', (
    tester,
  ) async {
    await boot(tester);

    expect(find.text('Ada Lovelace'), findsWidgets);

    // A rename through the manager — the same path the profile editor takes.
    di<MyProfileManager>().update(
      MyProfile(
        id: Id.fromWire(_accountId),
        fullName: FullName('Grace Hopper'),
        email: Email('ada@example.com'),
      ),
    );
    await tester.pump();

    // The greeting and the avatar both come from that one source, so the
    // change reaches the chrome without a restart (M12, Step 24).
    expect(find.text('Grace Hopper'), findsWidgets);
    expect(find.text('Ada Lovelace'), findsNothing);

    final avatar = tester.widget<UserAvatarWidget>(
      find.descendant(
        of: find.byType(CurrentUserAvatar),
        matching: find.byType(UserAvatarWidget),
      ),
    );
    expect(avatar.name, 'Grace Hopper');

    await unmountTree(tester);
  });
}
