import 'dart:async';

import 'package:dio/dio.dart' show ResponseBody;
import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/database.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_mobile/objectbox.g.dart' hide Id;

import '../../_support/pump_until_found.dart';
import '../../_support/stub_api.dart';

const _owner = '018f2b1e-0000-7000-8000-000000000001';
const _otherOwner = '018f2b1e-0000-7000-8000-000000000002';
const _broadcastId = '3f2b1c9e-8a4d-4f6b-9c1e-2d3a4b5c6d7e';
const _otherBroadcastId = '3f2b1c9e-8a4d-4f6b-9c1e-2d3a4b5c6d7f';

Broadcast _broadcast({String id = _broadcastId}) => Broadcast(
  id: Id.fromString(id),
  title: SingleLineString('Morning show'),
);

/// One row of `GET /broadcasts/bookmarks`, as the API sends it.
Map<String, dynamic> _item({
  String id = _broadcastId,
  String savedAt = '2026-10-28T09:15:00Z',
}) => {
  'id': id,
  'title': 'Morning show',
  'status': 'inactive',
  'state': 'ended',
  'totalParticipants': 2,
  'creatorId': _otherOwner,
  'creatorName': 'Ada Lovelace',
  'savedAt': savedAt,
};

/// The bookmarks page around [items].
Future<ResponseBody> _page(List<Map<String, dynamic>> items) async => envelope(
  data: {'data': items, 'next_cursor': null, 'has_next_page': false},
);

void main() {
  setUp(() => Command.globalExceptionHandler = (_, _) {});

  late Store store;
  late Database database;
  late BroadcastLocalService local;
  late StubApi api;
  late StreamController<BroadcastDeletedEvent> deleted;

  setUp(() {
    store = Store(
      getObjectBoxModel(),
      directory:
          'memory:meno-favourites-'
          '${DateTime.now().microsecondsSinceEpoch}',
    );
    database = Database.fromStore(store);
    local = BroadcastLocalService(
      Id.fromString(_owner),
      database,
      logger: MockMenoLogger(),
    );
    api = StubApi();
    deleted = StreamController<BroadcastDeletedEvent>.broadcast();
  });

  tearDown(() async {
    await deleted.close();
    await database.onDispose();
  });

  /// Builds a manager whose reads are answered with [items], every time.
  ///
  /// Two answers because the manager reads once as it is built and the test
  /// then awaits a read of its own; both are the same page, so the test is not
  /// asserting on which of the two arrived first.
  FavouritesManager build(List<Map<String, dynamic>> items) {
    api.answers
      ..add(() => _page(items))
      ..add(() => _page(items));

    final manager = FavouritesManager(
      local: local,
      http: BroadcastHttpService(stubClient(api)),
      logger: MockMenoLogger(),
      deletedBroadcasts: deleted.stream,
    );
    addTearDown(manager.onDispose);
    return manager;
  }

  test("the account's bookmarks are read from the API and mirrored", () async {
    final manager = build([_item()]);
    await manager.fetch.runAsync();

    expect(
      api.requests.first.path,
      '/broadcasts/bookmarks',
      reason: "the list is the account's, so it takes no id",
    );
    expect(manager.favourites, hasLength(1));
    expect(manager.favourites.first.broadcastId, Id.fromString(_broadcastId));
    expect(manager.favourites.first.title.orEmpty, 'Morning show');
    expect(
      local.isFavourited(Id.fromString(_broadcastId)),
      isTrue,
      reason: 'the star on a details screen reads the device copy',
    );
  });

  test('a broadcast the account no longer keeps is dropped from the device',
      () async {
    // What a stale copy looks like: the row is on disk but the account's list
    // no longer has it — the API is the record, so the read is the purge.
    await local.setFavourite(
      _broadcast(id: _otherBroadcastId),
      isFavourite: true,
    );
    expect(local.isFavourited(Id.fromString(_otherBroadcastId)), isTrue);

    final manager = build([_item()]);
    await manager.fetch.runAsync();

    expect(local.isFavourited(Id.fromString(_otherBroadcastId)), isFalse);
    expect(manager.favourites.map((f) => f.broadcastId), [
      Id.fromString(_broadcastId),
    ]);
  });

  test('a deleted broadcast is forgotten from the device copy', () async {
    final manager = build([_item()]);
    await manager.fetch.runAsync();
    expect(manager.favourites, hasLength(1));

    // The next read is what the server answers after the deletion.
    api.answers.add(() => _page([]));
    deleted.add(
      BroadcastDeletedEvent(broadcastId: Id.fromString(_broadcastId)),
    );

    await pumpUntil(() => !local.isFavourited(Id.fromString(_broadcastId)));
    await pumpUntil(() => manager.favourites.isEmpty);
  });

  test('another account keeping the same broadcast is not touched', () async {
    final other = BroadcastLocalService(
      Id.fromString(_otherOwner),
      database,
      logger: MockMenoLogger(),
    );
    await other.setFavourite(_broadcast(), isFavourite: true);

    final manager = build([]);
    await manager.fetch.runAsync();

    expect(
      other.isFavourited(Id.fromString(_broadcastId)),
      isTrue,
      reason: 'the mirror only replaces the account that owns it',
    );
    expect(manager.favourites, isEmpty);
  });
}
