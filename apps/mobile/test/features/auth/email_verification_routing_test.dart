import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/app.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';
import 'package:meno_mobile/locator.dart';
import 'package:shared_preferences/shared_preferences.dart';

const _config = Config(
  env: Environment.dev,
  menoApiUrl: 'https://dev.api',
  webSocketUrl: 'wss://dev.socket',
  menoLiveKitUrl: 'https://dev.livekit',
);

/// The account a registration leaves behind: signed in, address unconfirmed.
UserCredential _account({required bool verified}) => AuthResponseDto.fromJson({
  'access_token': 'access-token',
  'refresh_token': 'refresh-token',
  'expires_in': 3600,
  'user': {
    'id': '018f2b1e-0000-7000-8000-000000000000',
    'full_name': 'Jim Halpert',
    'email': 'jim@example.com',
    'verified': verified,
  },
}).toDomain;

/// Boots the app as an install that finished the intro, signed in as [account].
Future<void> bootWith(WidgetTester tester, UserCredential account) async {
  configureDependencies(_config);
  await di.allReady();

  await di<SessionStore>().saveActive(account);

  await tester.pumpWidget(const MenoApp());
  // Bounded pumps, not `pumpAndSettle`: the verification screen's code field
  // blinks a cursor, so the frame loop it drives never goes quiet.
  for (var frame = 0; frame < 20; frame++) {
    await tester.pump(const Duration(milliseconds: 20));
  }
}

void main() {
  setUp(() async {
    SharedPreferences.setMockInitialValues({
      StorageKeys.onboardingCompleted: true,
    });
    FlutterSecureStorage.setMockInitialValues({});
    await di.reset();
  });
  tearDown(di.reset);

  test('the vault keeps the verification flag', () async {
    configureDependencies(_config);
    await di.allReady();

    await di<SessionStore>().saveActive(_account(verified: false));
    final restored = await di<SessionStore>().readActiveCredential();

    expect(restored, isNotNull);
    expect(restored!.user.verified, isFalse);
  });

  testWidgets('an unverified account lands on the verification screen', (
    tester,
  ) async {
    await bootWith(tester, _account(verified: false));

    expect(find.text('Verify your email'), findsOneWidget);
    expect(find.widgetWithText(AppBar, 'Verify your email'), findsOneWidget);
  });

  testWidgets('a verified account lands in the app', (tester) async {
    await bootWith(tester, _account(verified: true));

    expect(find.text('Verify Your Email'), findsNothing);
    expect(find.text('Home'), findsWidgets);
  });
}
