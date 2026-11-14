import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/notification/notification.dart';

import '../../_support/stub_api.dart';

const _id = '11111111-1111-4111-8111-111111111111';

/// A proxy over an unread subscriber notification.
NotificationProxy _proxy() {
  return NotificationProxy(
    const Notification(
      id: _id,
      type: .userSubscribed,
      content: NotificationContent(subscriberName: 'Ada'),
    ),
  );
}

/// Registers a notifications service that answers from [api].
NotificationsHttpService _register(StubApi api) {
  final service = NotificationsHttpService(stubClient(api));
  di.registerSingleton<NotificationsHttpService>(service);
  return service;
}

void main() {
  setUp(() {
    // A command failure is asserted through the command, not printed.
    Command.globalExceptionHandler = (_, _) {};
  });

  tearDown(() async {
    await di.reset();
  });

  test('marking read flips the card and patches the row', () async {
    final api = StubApi([() async => envelope()]);
    _register(api);

    final proxy = _proxy();
    addTearDown(proxy.dispose);
    expect(proxy.isRead, isFalse);

    await proxy.markAsRead.runAsync();

    expect(proxy.isRead, isTrue);
    expect(api.requests.single.method, 'PATCH');
    expect(api.requests.single.path, '/notifications/$_id/read');
  });

  test('fresh data is newer than the local read decision', () async {
    final api = StubApi([() async => envelope()]);
    _register(api);

    final proxy = _proxy();
    addTearDown(proxy.dispose);

    await proxy.markAsRead.runAsync();
    expect(proxy.isRead, isTrue);

    proxy.notification = const Notification(
      id: _id,
      type: .userSubscribed,
      content: NotificationContent(subscriberName: 'Ada'),
    );

    expect(proxy.isRead, isFalse);
  });

  test('deleting asks the API for the row and keeps it for undo', () async {
    final api = StubApi([() async => envelope()]);
    _register(api);

    final proxy = _proxy();
    addTearDown(proxy.dispose);

    await proxy.delete.runAsync();

    expect(api.requests.single.method, 'DELETE');
    expect(api.requests.single.path, '/notifications/$_id');
  });

  test(
    'a failed delete leaves the row in place and reports the error',
    () async {
      final api = StubApi([
        () async => envelope(
          data: {'message': 'That notification does not exist'},
          status: 400,
        ),
      ]);
      _register(api);

      final proxy = _proxy();
      addTearDown(proxy.dispose);

      await expectLater(proxy.delete.runAsync(), throwsA(isA<MenoException>()));

      expect(proxy.notification.id, _id);
    },
  );
}
