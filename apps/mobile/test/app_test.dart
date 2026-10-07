import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/app.dart';

void main() {
  testWidgets('MenoApp renders the home screen', (tester) async {
    await tester.pumpWidget(const MenoApp());

    expect(find.byType(MaterialApp), findsOneWidget);
    expect(find.text('Hello World!'), findsOneWidget);
  });
}
