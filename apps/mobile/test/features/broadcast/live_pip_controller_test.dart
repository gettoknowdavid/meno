import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:pip_kit/pip_kit.dart';

import '../../_support/stub_api.dart';

const _accountId = '018f2b1e-0000-7000-8000-000000000000';
const _broadcastId = '018f2b1e-0000-7000-8000-0000000000aa';

Id get _account => Id.fromWire(_accountId);

/// A broadcast the creator of [_account] is hosting.
Broadcast _broadcast({bool hostedByAccount = true}) => Broadcast(
  id: Id.fromWire(_broadcastId),
  title: SingleLineString('Sunday Worship Live'),
  creator: Participant(
    id: hostedByAccount
        ? _account
        : Id.fromWire('018f2b1e-0000-7000-8000-0000000000bb'),
    fullName: SingleLineString('Ada Lovelace'),
  ),
);

/// `pip_kit`'s platform seam, with nothing behind it.
final class _FakePlatform extends PipPlatform {
  bool supported = true;
  int configureCalls = 0;
  int releaseCalls = 0;
  bool isOpen = false;

  void Function(bool isOpen)? _onStatus;
  void Function(String actionId)? _onAction;

  /// Taps a window button, as the system would.
  void tap(String id) => _onAction?.call(id);

  @override
  Future<bool> configure(Map<String, Object?> config) async {
    configureCalls++;
    return true;
  }

  @override
  Future<bool> enter() async {
    isOpen = true;
    _onStatus?.call(true);
    return true;
  }

  @override
  Future<bool> exit() async {
    isOpen = false;
    _onStatus?.call(false);
    return true;
  }

  @override
  Future<bool> isSupported() async => supported;

  @override
  Future<void> pushFrame(Uint8List rgba, int width, int height) async {}

  @override
  Future<void> release() async {
    releaseCalls++;
  }

  @override
  void setHandlers({
    required void Function(bool isOpen) onStatus,
    required void Function(String actionId) onAction,
  }) {
    _onStatus = onStatus;
    _onAction = onAction;
  }
}

void main() {
  late _FakePlatform platform;
  late PipController pip;
  late ValueNotifier<LiveSnapshot> snapshot;
  late ValueNotifier<String> elapsed;
  late List<String> exits;
  late List<bool> microphoneChanges;
  late LivePipController controller;

  LivePipController build({
    bool android = true,
    bool canPublish = true,
    bool isMicrophoneEnabled = true,
  }) {
    return controller = LivePipController(
      pip: pip,
      logger: MockMenoLogger(),
      snapshot: snapshot,
      accountId: _account,
      onExit: () async => exits.add('exit'),
      onMicrophoneChanged: (enabled) async => microphoneChanges.add(enabled),
      isAndroid: () => android,
    );
  }

  setUp(() {
    platform = _FakePlatform();
    pip = PipController.forTesting(
      platform,
      targetPlatform: TargetPlatform.android,
    );
    snapshot = ValueNotifier<LiveSnapshot>(
      LiveSnapshot(
        phase: LivePhase.live,
        broadcast: _broadcast(),
        canPublish: true,
        canEndBroadcast: true,
      ),
    );
    elapsed = ValueNotifier<String>('00:01:00');
    exits = [];
    microphoneChanges = [];
  });

  tearDown(() {
    controller.dispose();
    snapshot.dispose();
    elapsed.dispose();
  });

  Future<void> arm({
    bool canPublish = true,
    bool isMicrophoneEnabled = true,
    Broadcast? broadcast,
  }) => build().arm(
    broadcast: broadcast ?? _broadcast(),
    canPublish: canPublish,
    isMicrophoneEnabled: isMicrophoneEnabled,
    elapsed: elapsed,
  );

  test('arming prepares a 16:9 window that opens on leaving the app', () async {
    await arm();

    expect(controller.isArmed, isTrue);
    expect(pip.content.value, isNotNull);
    expect(pip.options.autoEnter, isTrue);
    expect(pip.options.aspectRatio, PipAspectRatio.landscape);
    expect(pip.options.actions.map((action) => action.id), [
      LivePipController.toggleMuteAction,
      LivePipController.leaveAction,
    ]);
  });

  test('a listener is offered the way out and nothing else', () async {
    await arm(canPublish: false);

    expect(pip.options.actions.map((action) => action.id), [
      LivePipController.leaveAction,
    ]);
  });

  test('the window is prepared once, however often it goes live', () async {
    await arm();
    final calls = platform.configureCalls;

    await controller.arm(
      broadcast: _broadcast(),
      canPublish: true,
      isMicrophoneEnabled: true,
      elapsed: elapsed,
    );

    expect(platform.configureCalls, calls, reason: 'arming is idempotent');
  });

  test('mute flips the action and the microphone it drives', () async {
    await arm();

    platform.tap(LivePipController.toggleMuteAction);
    await Future<void>.delayed(Duration.zero);

    expect(microphoneChanges, [false]);
    expect(pip.options.actions.first.label, 'Unmute');
    expect(
      pip.options.actions.first.icon,
      const PipActionIcon.drawable(LivePipController.mutedIcon),
    );
  });

  test("leave runs the room's own exit, not a second path", () async {
    await arm();

    platform.tap(LivePipController.leaveAction);
    await Future<void>.delayed(Duration.zero);

    expect(exits, ['exit']);
  });

  test('a device that cannot show a window is left alone', () async {
    platform.supported = false;
    await arm();

    expect(controller.isArmed, isFalse);
    expect(platform.configureCalls, 0);
  });

  test('another platform is left alone', () async {
    await build(android: false).arm(
      broadcast: _broadcast(),
      canPublish: true,
      isMicrophoneEnabled: true,
      elapsed: elapsed,
    );

    expect(controller.isArmed, isFalse);
    expect(platform.configureCalls, 0);
  });

  test('disarming closes an open window and forgets its content', () async {
    await arm();
    await pip.enter();
    platform.isOpen = true;

    await controller.disarm();

    expect(controller.isArmed, isFalse);
    expect(platform.isOpen, isFalse, reason: 'the window must be exited');
    expect(platform.releaseCalls, 1);
    expect(pip.content.value, isNull);
  });

  test('an action after the room closed does nothing', () async {
    await arm();
    await controller.disarm();

    platform.tap(LivePipController.leaveAction);
    platform.tap(LivePipController.toggleMuteAction);
    await Future<void>.delayed(Duration.zero);

    expect(exits, isEmpty);
    expect(microphoneChanges, isEmpty);
  });

  test("the window copy names the account's standing in the room", () async {
    await arm(canPublish: false, broadcast: _broadcast(hostedByAccount: false));

    expect(pip.options.title, 'Sunday Worship Live');
    expect(pip.options.subtitle, 'Ada Lovelace');
    expect(pip.options.actions.single.label, 'Leave');
  });

  test("the creator's leave action ends the broadcast", () async {
    await arm();

    expect(pip.options.actions.last.label, 'End broadcast');
  });
}
