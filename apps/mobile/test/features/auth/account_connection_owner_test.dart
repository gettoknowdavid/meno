import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/model/entities.dart';
import 'package:meno_mobile/_shared/services/socket_client.dart';
import 'package:meno_mobile/features/auth/auth.dart';

import '../../_support/fake_clock.dart';

/// Records the order of the owner's calls as one list.
final class _Calls {
  final List<String> entries = [];
}

final class _RecordingSocket implements SocketEndpoint {
  new(this.calls);

  final _Calls calls;
  int starts = 0;
  int stops = 0;

  @override
  bool get isOpen => starts > stops;

  @override
  Future<void> start() async {
    starts += 1;
    calls.entries.add('socket.start');
  }

  @override
  Future<void> stop() async {
    stops += 1;
    calls.entries.add('socket.stop');
  }
}

final class _RecordingAdapter implements ConnectionAdapter {
  new(this.calls);

  final _Calls calls;
  int installs = 0;
  int detaches = 0;

  @override
  void install() {
    installs += 1;
    calls.entries.add('adapter.install');
  }

  @override
  void detach() {
    detaches += 1;
    calls.entries.add('adapter.detach');
  }
}

/// A credential vault whose committed value the test can rotate.
final class _RotatingCredentials implements ConnectionCredentials {
  new(this.credential);

  UserCredential? credential;
  int reads = 0;

  @override
  Future<UserCredential?> readCredential(Id userId) async {
    reads += 1;
    return credential;
  }
}

final class _SilentLogger implements MenoLogger {
  @override
  void v(
    String message, {
    String? tag,
    Object? error,
    StackTrace? stackTrace,
  }) {}

  @override
  void d(
    String message, {
    String? tag,
    Object? error,
    StackTrace? stackTrace,
  }) {}

  @override
  void i(
    String message, {
    String? tag,
    Object? error,
    StackTrace? stackTrace,
  }) {}

  @override
  void w(
    String message, {
    String? tag,
    Object? error,
    StackTrace? stackTrace,
  }) {}

  @override
  void e(
    String message, {
    String? tag,
    Object? error,
    StackTrace? stackTrace,
  }) {}

  @override
  void fatal(
    String message, {
    String? tag,
    Object? error,
    StackTrace? stackTrace,
  }) {}

  @override
  void event(String name, Map<String, Object> context) {}
}

UserCredential _credential(String token, {DateTime? expiresAt}) =>
    UserCredential(
      session: Session(accessToken: token, expiresAt: expiresAt),
      user: User(
        id: Id.empty,
        fullName: FullName('Test'),
        email: Email('t@e.st'),
      ),
    );

void main() {
  final context = AccountContext(
    accountId: Id.fromString('11111111-1111-4111-8111-111111111111'),
    generation: 1,
  );
  final now = DateTime.utc(2026, 10, 28, 12);

  late _Calls calls;
  late _RecordingSocket socket;
  late _RecordingAdapter adapter;
  late _RotatingCredentials credentials;
  late FakeClock clock;
  late AccountConnectionOwner owner;

  setUp(() {
    calls = _Calls();
    socket = _RecordingSocket(calls);
    adapter = _RecordingAdapter(calls);
    credentials = _RotatingCredentials(_credential('token-a'));
    clock = FakeClock(now);
    owner = AccountConnectionOwner(
      context: context,
      socket: socket,
      adapters: [adapter],
      credentials: credentials,
      logger: _SilentLogger(),
      clock: clock,
    );
  });

  test('handlers are installed before the first connection attempt', () async {
    await owner.start(verified: true);

    expect(calls.entries, ['adapter.install', 'socket.start']);
    expect(adapter.installs, 1);
    expect(socket.starts, 1);
  });

  test('the token is read at every connect, never captured', () async {
    expect(await owner.accessToken(), 'token-a');

    credentials.credential = _credential('token-b');
    expect(await owner.accessToken(), 'token-b');

    // Two reads, two answers: the owner holds nothing between them.
    expect(credentials.reads, 2);

    credentials.credential = _credential(
      'token-c',
      expiresAt: now.subtract(const Duration(minutes: 1)),
    );
    expect(await owner.accessToken(), isNull);
  });

  test('a start after stop connects nothing', () async {
    await owner.start(verified: true);
    await owner.stop();
    calls.entries.clear();

    await owner.start(verified: true);

    expect(calls.entries, isEmpty);
    expect(socket.starts, 1);
    expect(adapter.installs, 1);
  });

  test(
    'an unverified account installs handlers but does not connect',
    () async {
      await owner.start(verified: false);

      expect(calls.entries, ['adapter.install']);
      expect(socket.starts, 0);
    },
  );

  test('stop is idempotent and detaches once', () async {
    await owner.start(verified: true);

    await Future.wait([owner.stop(), owner.stop()]);
    await owner.stop();

    expect(calls.entries, [
      'adapter.install',
      'socket.start',
      'socket.stop',
      'adapter.detach',
    ]);
    expect(socket.stops, 1);
    expect(adapter.detaches, 1);
    expect(owner.isStopping, isTrue);
  });
}
