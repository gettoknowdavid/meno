import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// Pumps a themed app around [child].
Future<void> _pump(WidgetTester tester, Widget child) async {
  await tester.pumpWidget(
    MaterialApp(
      theme: MTheme.light,
      home: Scaffold(body: Center(child: child)),
    ),
  );
}

/// A field wired to a local variable, so a tap's effect is observable.
class _Harness extends StatefulWidget {
  const new({this.onAction, this.validator});

  final VoidCallback? onAction;
  final FormFieldValidator<bool>? validator;

  @override
  State<_Harness> createState() => _HarnessState();
}

class _HarnessState extends State<_Harness> {
  bool value = false;

  bool get isChecked => value;

  @override
  Widget build(BuildContext context) {
    return MCheckboxFormField(
      label: 'By continuing you agree to our ',
      actionLabel: 'Terms of Service',
      onAction: widget.onAction,
      value: value,
      onChanged: (next) => setState(() => value = next),
      validator: widget.validator,
      autovalidateMode: AutovalidateMode.onUserInteraction,
    );
  }
}

void main() {
  group('MCheckboxFormField', () {
    testWidgets('starts unselected and toggles on a tap', (tester) async {
      await _pump(tester, const _Harness());

      expect(tester.widget<Checkbox>(find.byType(Checkbox)).value, isFalse);

      await tester.tap(find.byType(Checkbox));
      await tester.pump();

      expect(tester.widget<Checkbox>(find.byType(Checkbox)).value, isTrue);
    });

    testWidgets('tapping the label toggles the box too', (tester) async {
      await _pump(tester, const _Harness());

      await tester.tap(find.text('By continuing you agree to our '));
      await tester.pump();

      expect(tester.widget<Checkbox>(find.byType(Checkbox)).value, isTrue);

      // And back: the statement is a tap target in both directions.
      await tester.tap(find.text('By continuing you agree to our '));
      await tester.pump();

      expect(tester.widget<Checkbox>(find.byType(Checkbox)).value, isFalse);
    });

    testWidgets('the inline action does not toggle the box', (tester) async {
      var opened = 0;
      await _pump(tester, _Harness(onAction: () => opened++));

      await tester.tap(find.text('Terms of Service'));
      await tester.pump();

      expect(opened, 1);
      expect(tester.widget<Checkbox>(find.byType(Checkbox)).value, isFalse);
    });

    testWidgets('shows the validator message once the box is touched', (
      tester,
    ) async {
      await _pump(
        tester,
        _Harness(
          validator: (value) =>
              (value ?? false) ? null : 'You must accept to continue.',
        ),
      );

      expect(find.text('You must accept to continue.'), findsNothing);

      // Check, then clear: only then has the user interacted with the field.
      await tester.tap(find.text('By continuing you agree to our '));
      await tester.pump();
      await tester.tap(find.text('By continuing you agree to our '));
      await tester.pump();

      expect(find.text('You must accept to continue.'), findsOneWidget);
    });

    testWidgets('a disabled field ignores taps', (tester) async {
      await _pump(
        tester,
        MCheckboxFormField(
          label: 'Keep me signed in',
          value: false,
          enabled: false,
          onChanged: (_) => fail('a disabled field must not report a change'),
        ),
      );

      await tester.tap(find.text('Keep me signed in'));
      await tester.pump();

      expect(tester.widget<Checkbox>(find.byType(Checkbox)).value, isFalse);
    });
  });
}
