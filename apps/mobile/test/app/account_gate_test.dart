import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/app/account_gate.dart';
import 'package:meno_mobile/app/account_transition_owner.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:mocktail/mocktail.dart';

import '../_support/session_vault.dart';

const _a = '018f2b1e-0000-7000-8000-000000000001';
const _b = '018f2b1e-0000-7000-8000-000000000002';

class _MockLogger extends Mock implements MenoLogger;

CredentialRecord _account(String id) => testRecord(accountId: id);

final class _Tracked {
  new(this.events);

  final List<String> events;

  void dispose() => events.add('scope');
}

/// Records every mount and unmount, so a remount is observable.
class _Probe extends StatefulWidget {
  const new({required this.events});

  final List<String> events;

  @override
  State<_Probe> createState() => _ProbeState();
}

class _ProbeState extends State<_Probe> {
  @override
  void initState() {
    super.initState();
    widget.events.add('mount');
  }

  @override
  void dispose() {
    widget.events.add('widget');
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => const Text('account');
}

Future<void> _settle(WidgetTester tester, AccountTransitionOwner owner) async {
  for (var i = 0; i < 25 && !owner.snapshot.value.isReady; i++) {
    await tester.pump(const Duration(milliseconds: 20));
    // The owner's FIFO chain and the frame it awaits live in the real async
    // zone; the test binding's fake clock only advances frames.
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 5)),
    );
  }
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  late ValueNotifier<CredentialRecord?> credential;
  late AccountTransitionOwner owner;
  late List<String> events;

  setUp(() async {
    await di.reset();
    di.pushNewScope(scopeName: Scope.root);
    events = [];
    credential = ValueNotifier(null);
    owner = AccountTransitionOwner(
      activeRecord: credential,
      logger: _MockLogger(),
      register: (scope, account, context) {
        scope.registerSingleton<_Tracked>(
          _Tracked(events),
          dispose: (tracked) => tracked.dispose(),
        );
      },
    );
    await owner.init();
    await owner.restore();
    di.registerSingleton<AccountTransitionOwner>(owner);
  });

  tearDown(() async {
    await owner.onDispose();
    credential.dispose();
    await di.reset();
  });

  testWidgets('the signed-out branch performs no account lookup',
      (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: MTheme.light,
        home: AccountGate(child: _Probe(events: events)),
      ),
    );

    expect(find.text('account'), findsOneWidget);
    expect(events, ['mount']);
    expect(di.hasScope(Scope.authenticated), isFalse);
  });

  testWidgets('a generation change remounts the subtree', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: MTheme.light,
        home: AccountGate(child: _Probe(events: events)),
      ),
    );

    credential.value = _account(_a);
    await _settle(tester, owner);
    await tester.pump();
    final firstMounts = events.where((event) => event == 'mount').length;
    expect(firstMounts, greaterThanOrEqualTo(1));

    credential.value = _account(_b);
    await tester.pump();
    await _settle(tester, owner);
    await tester.pump();

    expect(
      events.where((event) => event == 'mount').length,
      greaterThan(firstMounts),
      reason: 'the new generation must build a fresh subtree',
    );
    expect(find.text('account'), findsOneWidget);
  });

  testWidgets('the old subtree is disposed before its scope is popped',
      (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: MTheme.light,
        home: AccountGate(child: _Probe(events: events)),
      ),
    );

    credential.value = _account(_a);
    await _settle(tester, owner);
    await tester.pump();
    events.clear();

    credential.value = _account(_b);
    await tester.pump();
    for (var i = 0; i < 25; i++) {
      await tester.pump(const Duration(milliseconds: 20));
    }

    final widgetDisposed = events.indexOf('widget');
    final scopePopped = events.indexOf('scope');
    expect(widgetDisposed, greaterThanOrEqualTo(0));
    expect(scopePopped, greaterThanOrEqualTo(0));
    expect(
      widgetDisposed,
      lessThan(scopePopped),
      reason: 'the frame is the signal that the old subtree is gone',
    );
  });
}
