import 'dart:convert';

import 'package:flutter_it/flutter_it.dart' show Command;
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/manager/user_manager.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_mobile/features/onboarding/manager/onboarding_manager.dart';
import 'package:meno_mobile/features/onboarding/services/onboarding_service.dart';
import 'package:meno_mobile/features/profile/profile.dart';
import 'package:meno_mobile/features/settings/settings.dart';
import 'package:meno_mobile/locator.dart';
import 'package:mocktail/mocktail.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '_support/fake_path_provider.dart';
import '_support/pump_until_found.dart';
import '_support/session_vault.dart';

class MockMenoLogger extends Mock implements MenoLogger;

/// The account the sign-in flow would have left in the vault.
const String _accountId = '018f2b1e-0000-7000-8000-000000000001';

final CredentialRecord _account = testRecord(accountId: _accountId);

/// Files [_account] as the signed-in account, greeting included.
Future<void> signIn() => seedVault(
  _account,
  remembered: testGreeting(
    accountId: _accountId,
    displayName: 'Ada Lovelace',
    email: 'ada@example.com',
  ),
);

const _config = Config(
  env: Environment.dev,
  menoApiUrl: 'https://dev.api',
  webSocketUrl: 'wss://dev.socket',
  menoLiveKitUrl: 'https://dev.livekit',
);

void main() {
  // The container reaches the platform channels (secure storage, preferences)
  // while it starts, so the binding has to exist before it is asked to.
  TestWidgetsFlutterBinding.ensureInitialized();

  setUp(() async {
    installFakePathProvider();
    SharedPreferences.setMockInitialValues({});
    FlutterSecureStorage.setMockInitialValues({});
    await di.reset();

    // `main` registers one of these before the app runs; without it a command
    // that fails in debug asserts instead of routing its error anywhere.
    Command.globalExceptionHandler = (error, stackTrace) {};
  });
  tearDown(() {
    Command.globalExceptionHandler = null;
    return di.reset();
  });

  group('configureDependencies', () {
    test('registers config, logger and secure storage in the root scope', () {
      configureDependencies(_config);

      expect(di.currentScopeName, Scope.root);
      expect(di<Config>(), same(_config));
      expect(di<MenoLogger>(), isA<MenoLoggerImpl>());
      expect(di.isRegistered<FlutterSecureStorage>(), isTrue);
    });

    test('registers the onboarding storage and manager', () async {
      configureDependencies(_config);
      await di.allReady();

      expect(di.isRegistered<SharedPreferences>(), isTrue);
      expect(di.isRegistered<OnboardingService>(), isTrue);
      expect(di<OnboardingManager>().isOnboarded.value, isFalse);
    });

    test('a stored onboarding flag reaches the manager', () async {
      SharedPreferences.setMockInitialValues({
        StorageKeys.onboardingCompleted: true,
      });
      configureDependencies(_config);
      await di.allReady();

      expect(di<OnboardingService>().isOnboarded, isTrue);
      expect(di<OnboardingManager>().isOnboarded.value, isTrue);
    });

    test("the theme manager follows the account's display setting", () async {
      configureDependencies(_config);
      await di.allReady();

      final theme = di<ThemeManager>();
      // Nobody is signed in, so nothing has asked for a theme: the device's own
      // appearance is what the sign-in screens are drawn in.
      expect(theme.themeMode.value, ThemeMode.system);

      // The account's own choice, as the device cached it last time. Signing in
      // reads it, so the app is painted in it before any screen asks.
      await di<LocalStorage>().setString(
        'account.v2/$_accountId/settings',
        value: jsonEncode(
          const UserSettings(display: UserDisplay.dark).toDto.toJson(),
        ),
      );

      await signIn();
      await pumpUntil(() => di.currentScopeName == Scope.authenticated);
      await pumpUntil(() => theme.themeMode.value == ThemeMode.dark);

      expect(di<SettingsManager>().settings.value.display, UserDisplay.dark);
      expect(theme.themeMode.value, ThemeMode.dark);
    });

    test('signing out leaves the appearance alone', () async {
      configureDependencies(_config);
      await di.allReady();

      await di<LocalStorage>().setString(
        'account.v2/$_accountId/settings',
        value: jsonEncode(
          const UserSettings(display: UserDisplay.dark).toDto.toJson(),
        ),
      );

      await signIn();
      await pumpUntil(() => di.currentScopeName == Scope.authenticated);

      final theme = di<ThemeManager>();
      await pumpUntil(() => theme.themeMode.value == ThemeMode.dark);
      expect(theme.themeMode.value, ThemeMode.dark);

      // The account's own settings go with the account, but the theme is the
      // device's: repainting the sign-in screen back to the system theme would
      // be a change nobody asked for.
      await vault.deactivate();
      await pumpUntil(() => di.currentScopeName == Scope.root);

      expect(theme.themeMode.value, ThemeMode.dark);
    });

    test('turning dark mode on repaints the app', () async {
      configureDependencies(_config);
      await di.allReady();

      await signIn();
      await pumpUntil(() => di.currentScopeName == Scope.authenticated);

      final theme = di<ThemeManager>();
      expect(di<UserManager>().currentUserId.value, Id.fromString(_accountId));
      expect(theme.themeMode.value, ThemeMode.system);

      // The repaint does not wait for the write: the switch feels instant.
      di<SettingsManager>().toggleDarkMode.run(true);
      await pumpUntil(() => theme.themeMode.value == ThemeMode.dark);
      expect(theme.themeMode.value, ThemeMode.dark);

      // Nothing answers the write in a test, so the command rolls the
      // optimistic change back. Waited for here, so that the failure belongs to
      // this test rather than to whichever one runs next.
      await pumpUntil(() => theme.themeMode.value == ThemeMode.system);
      expect(theme.themeMode.value, ThemeMode.system);
    });

    test("the account's own services exist only while it is signed in",
        () async {
      configureDependencies(_config);
      await di.allReady();

      expect(di.isRegistered<MyProfileManager>(), isFalse);
      expect(di.isRegistered<SettingsManager>(), isFalse);
      expect(di.isRegistered<ActiveBroadcastManager>(), isFalse);
      expect(di.isRegistered<BroadcastEditorManager>(), isFalse);

      await signIn();
      await pumpUntil(() => di.currentScopeName == Scope.authenticated);

      // Registered and already holding the account's own state, so a screen
      // built on this frame shows the cached record rather than nothing.
      expect(di<MyProfileManager>().profile.value.isEmpty, isTrue);
      expect(di<SettingsManager>().settings.value.display, UserDisplay.system);
      expect(di.isRegistered<ActiveBroadcastManager>(), isTrue);
      expect(di.isRegistered<LiveSessionStore>(), isTrue);
      // Widget-owned since Step 34: the editor exists only while its page does.
      expect(di.isRegistered<BroadcastEditorManager>(), isFalse);

      await vault.deactivate();
      // The scope's name changes before its registrations are torn down, so the
      // wait is on the things the assertions are about.
      await pumpUntil(
        () => !di.isRegistered<MyProfileManager>(),
        step: const Duration(milliseconds: 10),
      );

      expect(di.isRegistered<MyProfileManager>(), isFalse);
      expect(di.isRegistered<SettingsManager>(), isFalse);
      expect(di.isRegistered<ActiveBroadcastManager>(), isFalse);
      expect(di.isRegistered<LiveSessionStore>(), isFalse);
    });

    test('an account that signs in is scoped, with the session inside it',
        () async {
      configureDependencies(_config);
      await di.allReady();
      expect(di.currentScopeName, Scope.root);

      await signIn();
      await pumpUntil(() => di.currentScopeName == Scope.authenticated);

      expect(di<CredentialRecord>().accountId, Id.fromString(_accountId));
      expect(di.isRegistered<ActiveBroadcastManager>(), isTrue);

      await vault.deactivate();
      await pumpUntil(
        () => !di.isRegistered<ActiveBroadcastManager>(),
        step: const Duration(milliseconds: 10),
      );

      expect(di.isRegistered<ActiveBroadcastManager>(), isFalse);
    });

    test('wires the logger policy to the config environment', () {
      const prodConfig = Config(
        env: Environment.prod,
        menoApiUrl: 'https://api',
        webSocketUrl: 'wss://socket',
        menoLiveKitUrl: 'https://livekit',
      );

      configureDependencies(prodConfig);

      final logger = di<MenoLogger>() as MenoLoggerImpl;
      expect(logger.env, Environment.prod);
    });
  });

  group('DI with a mocked logger', () {
    test('resolves the mock and verifies startup interactions', () {
      final logger = MockMenoLogger();
      di.registerSingleton<MenoLogger>(logger);

      di<MenoLogger>()
        ..i('App starting', tag: MenoLogTag.lifecycle.label)
        ..event('app_start', {'env': 'dev'});

      verify(() => logger.i('App starting', tag: MenoLogTag.lifecycle.label))
          .called(1);
      verify(() => logger.event('app_start', {'env': 'dev'})).called(1);
      verifyNoMoreInteractions(logger);
    });
  });
}
