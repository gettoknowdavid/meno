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

import '_support/pump_until_found.dart';

class MockMenoLogger extends Mock implements MenoLogger;

/// The account the sign-in flow would have left in the store.
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
        StorageKeys.forUser(StorageKeys.settings, '1'),
        value: jsonEncode(
          const UserSettings(display: UserDisplay.dark).toDto.toJson(),
        ),
      );

      await di<SessionStore>().saveActive(_account);
      await pumpUntil(() => di.currentScopeName == Scope.authenticated);
      await pumpUntil(() => theme.themeMode.value == ThemeMode.dark);

      expect(di<SettingsManager>().settings.value.display, UserDisplay.dark);
      expect(theme.themeMode.value, ThemeMode.dark);
    });

    test('signing out leaves the appearance alone', () async {
      configureDependencies(_config);
      await di.allReady();

      await di<LocalStorage>().setString(
        StorageKeys.forUser(StorageKeys.settings, '1'),
        value: jsonEncode(
          const UserSettings(display: UserDisplay.dark).toDto.toJson(),
        ),
      );

      await di<SessionStore>().saveActive(_account);
      await pumpUntil(() => di.currentScopeName == Scope.authenticated);

      final theme = di<ThemeManager>();
      await pumpUntil(() => theme.themeMode.value == ThemeMode.dark);
      expect(theme.themeMode.value, ThemeMode.dark);

      // The account's own settings go with the account, but the theme is the
      // device's: repainting the sign-in screen back to the system theme would
      // be a change nobody asked for.
      await di<SessionStore>().clearActive();
      await pumpUntil(() => di.currentScopeName == Scope.root);

      expect(theme.themeMode.value, ThemeMode.dark);
    });

    test('turning dark mode on repaints the app', () async {
      configureDependencies(_config);
      await di.allReady();

      await di<SessionStore>().saveActive(_account);
      await pumpUntil(() => di.currentScopeName == Scope.authenticated);

      final theme = di<ThemeManager>();
      expect(di<UserManager>().currentUserId.value, '1');
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
      expect(di.isRegistered<StreamManager>(), isFalse);
      expect(di.isRegistered<LiveScopeManager>(), isFalse);

      await di<SessionStore>().saveActive(_account);
      await pumpUntil(() => di.currentScopeName == Scope.authenticated);

      // Registered and already holding the account's own state, so a screen
      // built on this frame shows the cached record rather than nothing.
      expect(di<MyProfileManager>().profile.value.isEmpty, isTrue);
      expect(di<SettingsManager>().settings.value.display, UserDisplay.system);
      expect(di.isRegistered<StreamManager>(), isTrue);
      expect(di.isRegistered<LiveSessionStore>(), isTrue);
      expect(di.isRegistered<LiveScopeManager>(), isTrue);

      await di<SessionStore>().clearActive();
      await pumpUntil(() => di.currentScopeName == Scope.root);

      expect(di.isRegistered<MyProfileManager>(), isFalse);
      expect(di.isRegistered<SettingsManager>(), isFalse);
      expect(di.isRegistered<StreamManager>(), isFalse);
      expect(di.isRegistered<LiveScopeManager>(), isFalse);
    });

    test('an account that signs in is scoped, with the session inside it',
        () async {
      configureDependencies(_config);
      await di.allReady();
      expect(di.currentScopeName, Scope.root);

      await di<SessionStore>().saveActive(_account);
      await pumpUntil(() => di.currentScopeName == Scope.authenticated);

      expect(di<UserCredential>().userId, '1');
      expect(di.isRegistered<LiveScopeManager>(), isTrue);

      await di<SessionStore>().clearActive();
      await pumpUntil(() => di.currentScopeName == Scope.root);

      expect(di.isRegistered<LiveScopeManager>(), isFalse);
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
