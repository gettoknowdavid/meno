import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/room_transport.dart';

void main() {
  test('a silent room reports the connection it goes through', () async {
    final room = SilentRoomTransport();
    addTearDown(room.onDispose);

    final states = <RoomConnectionState>[];
    room.connectionState.listen(states.add);

    expect(room.isConnected, isFalse);

    await room.connect('room-token', canPublish: true);
    expect(room.isConnected, isTrue);

    await room.disconnect();
    expect(room.isConnected, isFalse);

    // A broadcast stream delivers on a later microtask, so the last event is
    // only in [states] once the queue has run.
    await pumpEventQueue();

    expect(states, [
      RoomConnectionState.connecting,
      RoomConnectionState.connected,
      RoomConnectionState.disconnected,
    ]);
  });

  test('a host joins publishing and an audience joins muted', () async {
    final host = SilentRoomTransport();
    addTearDown(host.onDispose);
    await host.connect('host-token', canPublish: true);
    expect(host.isMicrophoneEnabled, isTrue);

    final listener = SilentRoomTransport();
    addTearDown(listener.onDispose);
    await listener.connect('listener-token', canPublish: false);
    expect(listener.isMicrophoneEnabled, isFalse);
  });

  test('the microphone switches while the room is up', () async {
    final room = SilentRoomTransport();
    addTearDown(room.onDispose);
    await room.connect('room-token', canPublish: true);

    await room.setMicrophoneEnabled(false);
    expect(room.isMicrophoneEnabled, isFalse);

    await room.setMicrophoneEnabled(true);
    expect(room.isMicrophoneEnabled, isTrue);
  });

  test('the microphone cannot be switched outside a room', () async {
    final room = SilentRoomTransport();
    addTearDown(room.onDispose);

    await expectLater(
      room.setMicrophoneEnabled(true),
      throwsA(isA<LiveKitNotConnected>()),
    );
  });

  test('hanging up twice is a no-op', () async {
    final room = SilentRoomTransport();
    addTearDown(room.onDispose);

    await room.disconnect();
    await room.disconnect();

    expect(room.isConnected, isFalse);
  });

  test(
    'a join can be made to take time, for watching the transition',
    () async {
      final room = SilentRoomTransport(
        connectDelay: const Duration(milliseconds: 50),
      );
      addTearDown(room.onDispose);

      final connecting = room.connect('room-token', canPublish: false);
      expect(room.isConnected, isFalse);
      expect(room.isMicrophoneEnabled, isFalse);

      await connecting;
      expect(room.isConnected, isTrue);
    },
  );
}
