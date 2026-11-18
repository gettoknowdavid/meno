import 'dart:async';

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

Broadcast _broadcast() => Broadcast(
  id: Id.fromString(_broadcastId),
  title: SingleLineString('Morning show'),
);

void main() {
  setUp(() => Command.globalExceptionHandler = (_, _) {});

  late Store store;
  late Database database;
  late BroadcastLocalService local;
  late FavouritesManager manager;
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
    deleted = StreamController<BroadcastDeletedEvent>.broadcast();
    manager = FavouritesManager(
      local: local,
      logger: MockMenoLogger(),
      deletedBroadcasts: deleted.stream,
    );
  });

  tearDown(() async {
    await manager.onDispose();
    await deleted.close();
    await database.onDispose();
  });

  test('a deleted broadcast is forgotten from the local store', () async {
    await local.setFavourite(_broadcast(), isFavourite: true);
    manager.fetch.run();
    expect(manager.favourites, hasLength(1));

    deleted.add(
      BroadcastDeletedEvent(broadcastId: Id.fromString(_broadcastId)),
    );

    await pumpUntil(() => manager.favourites.isEmpty);
    expect(local.isFavourited(Id.fromString(_broadcastId)), isFalse);
  });

  test('another account keeping the same broadcast is not touched', () async {
    final other = BroadcastLocalService(
      Id.fromString(_otherOwner),
      database,
      logger: MockMenoLogger(),
    );
    await other.setFavourite(_broadcast(), isFavourite: true);
    await local.setFavourite(_broadcast(), isFavourite: true);
    manager.fetch.run();

    deleted.add(
      BroadcastDeletedEvent(broadcastId: Id.fromString(_broadcastId)),
    );

    await pumpUntil(() => manager.favourites.isEmpty);
    expect(local.isFavourited(Id.fromString(_broadcastId)), isFalse);
    expect(
      other.isFavourited(Id.fromString(_broadcastId)),
      isTrue,
      reason: 'the store removes only the account that owns the row',
    );
  });

  test('a manager built after a save already holds the list', () async {
    // What the favourites tab sees on a fresh launch: the row is on disk and
    // nobody has written anything since, so the read has to happen by itself.
    await local.setFavourite(_broadcast(), isFavourite: true);

    final rebuilt = FavouritesManager(
      local: local,
      logger: MockMenoLogger(),
      deletedBroadcasts: deleted.stream,
    );
    addTearDown(rebuilt.onDispose);

    expect(rebuilt.favourites, hasLength(1));
  });

  test('a save made elsewhere refreshes a manager already listening', () async {
    expect(manager.favourites, isEmpty);

    // The star on a details screen writes through this service, not through
    // the manager: the store is what tells the manager its copy is stale.
    await local.setFavourite(_broadcast(), isFavourite: true);

    await pumpUntil(() => manager.favourites.isNotEmpty);
    expect(
      manager.favourites.first.broadcastId,
      Id.fromString(_broadcastId),
    );
  });
}
