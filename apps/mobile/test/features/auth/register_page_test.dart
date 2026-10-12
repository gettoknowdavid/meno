import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/app.dart';
import 'package:meno_mobile/features/auth/manager/auth_manager.dart';
import 'package:meno_mobile/features/auth/manager/register_form_manager.dart';
import 'package:meno_mobile/locator.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../../_support/pump_until_found.dart';

const _config = Config(
  env: Environment.dev,
  menoApiUrl: 'https://dev.api',
  webSocketUrl: 'wss://dev.socket',
  menoLiveKitUrl: 'https://dev.livekit',
);

const _termsLabel = 'By continuing you agree to our ';

void main() {
  setUp(() async {
    SharedPreferences.setMockInitialValues({});
    FlutterSecureStorage.setMockInitialValues({});
    await di.reset();
  });
  tearDown(di.reset);

  /// Opens the register form from the intro, where a fresh install starts.
  Future<void> openRegisterForm(WidgetTester tester) async {
    configureDependencies(_config);
    // `main()` does this after the locator is up. Without it a failure the
    // filter routes to the global handler trips its "no handler registered"
    // assertion instead of being reported.
    configureGlobalExceptionHandler();

    await tester.pumpWidget(const MenoApp());
    await pumpUntilFound(tester, find.text('Get started'));

    await tester.ensureVisible(find.text('Get started'));
    await tester.pump();
    await tester.tap(find.text('Get started'));
    await tester.pumpAndSettle();
  }

  Finder registerButton() => find.byKey(const Key('register_submit'));

  Checkbox termsBox(WidgetTester tester) =>
      tester.widget<Checkbox>(find.byType(Checkbox));

  bool isRegisterDisabled(WidgetTester tester) =>
      tester.widget<MPrimaryButton>(registerButton()).disabled;

  Future<void> fillValidForm(WidgetTester tester) async {
    final fields = find.byType(MTextFormField);
    await tester.enterText(fields.at(0), 'Jim Halpert');
    await tester.enterText(fields.at(1), 'jim@example.com');
    await tester.enterText(
      find.byKey(const Key('register_form_password_input')),
      'Password1!',
    );
    await tester.pumpAndSettle();
  }

  testWidgets('the terms box starts unchecked and parks the submit button', (
    tester,
  ) async {
    await openRegisterForm(tester);

    expect(find.byType(Checkbox), findsOneWidget);
    expect(termsBox(tester).value, isFalse);
    expect(isRegisterDisabled(tester), isTrue);

    await unmountTree(tester);
  });

  testWidgets('a valid form stays parked until the terms box is checked', (
    tester,
  ) async {
    await openRegisterForm(tester);
    await fillValidForm(tester);

    expect(isRegisterDisabled(tester), isTrue);

    await tester.tap(find.byType(Checkbox));
    await tester.pumpAndSettle();

    expect(termsBox(tester).value, isTrue);
    expect(isRegisterDisabled(tester), isFalse);

    await unmountTree(tester);
  });

  testWidgets('the statement beside the box is a tap target', (tester) async {
    await openRegisterForm(tester);

    await tester.tap(find.text(_termsLabel));
    await tester.pumpAndSettle();

    expect(termsBox(tester).value, isTrue);

    await tester.tap(find.text('Terms of Service'));
    await tester.pumpAndSettle();

    expect(termsBox(tester).value, isTrue);

    await unmountTree(tester);
  });

  testWidgets('a failed registration leaves the form ready to resubmit', (
    tester,
  ) async {
    await openRegisterForm(tester);
    await fillValidForm(tester);
    await tester.tap(find.text(_termsLabel));
    await tester.pumpAndSettle();
    expect(isRegisterDisabled(tester), isFalse);

    // The submit button sits below the fold of the default test window.
    await tester.ensureVisible(registerButton());
    await tester.pump();

    // Every request fails in a widget test — the binding answers 400 — so this
    // is the "address already registered" case: a refusal, not a success.
    await tester.tap(registerButton());
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));

    // The command really did fail — the binding answers 400 to every request —
    // and the failure is a transport one, not a rejected form.
    final result = di<AuthManager>().register.results.value;
    expect(result.isSuccess, isFalse);
    expect(result.error, isA<MenoException>());
    // What was typed is still held and still submittable, so the user corrects
    // the field the API complained about instead of all of them.
    final manager = di<RegisterFormManager>();
    expect(manager.fullName.value.isValid, isTrue);
    expect(manager.email.value.getOrNull, 'jim@example.com');
    expect(manager.password.value.isValid, isTrue);
    expect(manager.terms.value.input, isTrue);
    expect(manager.isValid.value, isTrue);

    expect(find.text('jim@example.com'), findsOneWidget);
    expect(termsBox(tester).value, isTrue);
    expect(isRegisterDisabled(tester), isFalse);

    await unmountTree(tester);
  });
}
