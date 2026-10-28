import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';

const _creator = 'c0ffee00-1111-4222-8333-444455556666';
const _cohost = 'c0ffee00-2222-4222-8333-444455556666';
const _listener = 'c0ffee00-3333-4222-8333-444455556666';

Broadcast _broadcast({String creatorId = _creator}) => Broadcast(
  id: Id.fromString('3f2b1c9e-8a4d-4f6b-9c1e-2d3a4b5c6d7e'),
  title: SingleLineString('Morning show'),
  state: .live,
  status: .active,
  creator: Participant(
    id: Id.fromString(creatorId),
    fullName: SingleLineString('Ada'),
  ),
);

void main() {
  group('capabilities', () {
    test('a cohost may publish and may not end the broadcast', () {
      final snapshot = LiveSnapshot.room(
        phase: .live,
        broadcast: _broadcast(),
        accountId: Id.fromString(_cohost),
        canPublish: true,
        role: .cohost,
      );

      expect(snapshot.canPublish, isTrue);
      expect(snapshot.canEndBroadcast, isFalse);
      expect(snapshot.leavesOnly, isTrue);
      expect(snapshot.exitsByEnding, isFalse);
    });

    test('the creator may end it, whatever the role says', () {
      final snapshot = LiveSnapshot.room(
        phase: .live,
        broadcast: _broadcast(),
        accountId: Id.fromString(_creator),
        canPublish: true,
        role: .host,
      );

      expect(snapshot.canPublish, isTrue);
      expect(snapshot.canEndBroadcast, isTrue);
      expect(snapshot.exitsByEnding, isTrue);
      expect(snapshot.leavesOnly, isFalse);
    });

    test('a listener can do neither, and leaving is not ending', () {
      final snapshot = LiveSnapshot.room(
        phase: .live,
        broadcast: _broadcast(),
        accountId: Id.fromString(_listener),
        canPublish: false,
        role: .listener,
      );

      expect(snapshot.canPublish, isFalse);
      expect(snapshot.canEndBroadcast, isFalse);
      expect(snapshot.isInRoom, isTrue);
      expect(snapshot.leavesOnly, isTrue);
    });
  });

  group('snapshots', () {
    test('an identical re-publish notifies nobody', () {
      final snapshot = LiveSnapshot.room(
        phase: .live,
        broadcast: _broadcast(),
        accountId: Id.fromString(_creator),
        canPublish: true,
      );
      final notifier = ValueNotifier<LiveSnapshot>(snapshot);
      addTearDown(notifier.dispose);

      var notifications = 0;
      notifier.addListener(() => notifications++);

      notifier.value = LiveSnapshot.room(
        phase: .live,
        broadcast: _broadcast(),
        accountId: Id.fromString(_creator),
        canPublish: true,
      );
      expect(notifications, 0);

      notifier.value = snapshot.copyWith(phase: .reconnecting);
      expect(notifications, 1);
    });

    test('the ended summary survives a reset of the rest of the state', () {
      final summary = _broadcast();
      final ended = LiveSnapshot(
        phase: .ended,
        endedSummary: summary,
        endReason: .normal,
      );

      final idle = ended.copyWith(phase: .idle, broadcast: null);

      expect(idle.phase, LivePhase.idle);
      expect(idle.endedSummary, summary);
      expect(idle.endReason, EndReason.normal);
    });
  });
}
