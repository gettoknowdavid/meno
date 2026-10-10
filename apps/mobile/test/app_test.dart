import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/app.dart';
import 'package:meno_mobile/locator.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '_support/pump_until_found.dart';

void main() {
  setUp(() async {
    SharedPreferences.setMockInitialValues({});
    await di.reset();
  });
  tearDown(di.reset);

  testWidgets('MenoApp boots into the router and lands on onboarding', (
    tester,
  ) async {
    configureDependencies(
      const Config(
        env: Environment.dev,
        menoApiUrl: 'https://dev.api',
        webSocketUrl: 'wss://dev.socket',
        menoLiveKitUrl: 'https://dev.livekit',
      ),
    );

    await tester.pumpWidget(const MenoApp());
    await pumpUntilFound(tester, find.text('Get started'));

    expect(find.byType(MaterialApp), findsOneWidget);
    expect(find.text('Get started'), findsOneWidget);

    await unmountTree(tester);
  });
}
