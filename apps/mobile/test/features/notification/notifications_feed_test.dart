import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/features/notification/notification.dart';

import '../../_support/stub_api.dart';

/// Four ids the API could have minted.
const _todayId = '11111111-1111-4111-8111-111111111111';
const _weekId = '22222222-2222-4222-8222-222222222222';
const _oldId = '33333333-3333-4333-8333-333333333333';
const _nextId = '44444444-4444-4444-8444-444444444444';

/// One notification as `GET /notifications` serialises it.
Map<String, dynamic> _item({
  required String id,
  String typeCode = 'user_subscribed',
  DateTime? createdAt,
  bool read = false,
}) {
  return {
    'id': id,
    'typeCode': typeCode,
    'title': 'New subscriber',
    'body': 'Ada just subscribed to you',
    'read': read,
    'createdAt': (createdAt ?? DateTime.now()).toIso8601String(),
    'deepLink': 'meno://profile/$id',
  };
}

/// One page in the API's cursor envelope.
Map<String, dynamic> _page(
  List<Map<String, dynamic>> items, {
  String? nextCursor,
  bool hasNextPage = false,
}) {
  return {
    'data': items,
    'next_cursor': nextCursor,
    'has_next_page': hasNextPage,
  };
}

/// A feed over [api], with the real HTTP service the app uses.
NotificationsFeedSource _feed(StubApi api) {
  return NotificationsFeedSource(NotificationsHttpService(stubClient(api)));
}

void main() {
  test('reads the first page, then follows its cursor', () async {
    final api = StubApi([
      () async => envelope(
        data: _page(
          [_item(id: _todayId)],
          nextCursor: 'cursor-1',
          hasNextPage: true,
        ),
      ),
      () async => envelope(data: _page([_item(id: _nextId)])),
    ]);
    final feed = _feed(api);
    addTearDown(feed.onDispose);

    await feed.refresh();

    expect(feed.items, hasLength(1));
    expect(feed.items.single.notification.id, _todayId);
    expect(feed.hasNextPage, isTrue);
    expect(feed.updateWasCalled, isTrue);

    await feed.requestNextPageCommand.runAsync();

    expect(feed.items, hasLength(2));
    expect(feed.hasNextPage, isFalse);
    expect(api.requests[1].queryParameters['cursor'], 'cursor-1');
    expect(
      api.requests[1].queryParameters['limit'],
      NotificationsHttpService.defaultLimit,
    );
  });

  test('a row the decoder cannot read does not sink the page', () async {
    final api = StubApi([
      () async => envelope(
        data: _page([
          _item(id: _todayId, typeCode: 'broadcast_ended'),
          _item(id: _weekId),
        ]),
      ),
    ]);
    final feed = _feed(api);
    addTearDown(feed.onDispose);

    await feed.refresh();

    // `broadcast_ended` has no card in this build, so the row is skipped and
    // the one beside it still arrives.
    expect(feed.items, hasLength(1));
    expect(feed.items.single.notification.id, _weekId);
  });

  test(
    'groups the inbox into today, this week and older, newest first',
    () async {
      final now = DateTime.now();
      final api = StubApi([
        () async => envelope(
          data: _page([
            _item(
              id: _oldId,
              createdAt: now.subtract(const Duration(days: 40)),
            ),
            _item(id: _todayId, createdAt: now),
            _item(
              id: _weekId,
              createdAt: now.subtract(const Duration(days: 3)),
            ),
          ]),
        ),
      ]);
      final feed = _feed(api);
      addTearDown(feed.onDispose);

      await feed.refresh();

      final rows = feed.groupedItems.value;
      final labels = rows
          .whereType<NotificationSectionHeader>()
          .map((header) => header.label)
          .toList();
      final ids = rows
          .whereType<NotificationListEntry>()
          .map((entry) => entry.proxy.notification.id)
          .toList();

      expect(labels, ['Today', 'This week', 'Older']);
      expect(ids, [_todayId, _weekId, _oldId]);
    },
  );

  test('a refresh keeps one proxy per row and updates it', () async {
    final api = StubApi([
      () async => envelope(data: _page([_item(id: _todayId)])),
      () async => envelope(data: _page([_item(id: _todayId, read: true)])),
    ]);
    final feed = _feed(api);
    addTearDown(feed.onDispose);

    await feed.refresh();
    final first = feed.items.single;

    await feed.refresh();

    expect(feed.items, hasLength(1));
    expect(identical(feed.items.single, first), isTrue);
    expect(feed.items.single.notification.read, isTrue);
  });

  test(
    'an unread-only read sends the filter the endpoint understands',
    () async {
      final api = StubApi([() async => envelope(data: _page([]))]);

      await NotificationsHttpService(stubClient(api))
          .getNotifications(unreadOnly: true);

      expect(api.requests.single.queryParameters['unreadOnly'], isTrue);
    },
  );
}
