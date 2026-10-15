import 'package:meno_mobile/_shared/services/database.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';
import 'package:meno_mobile/features/broadcast/services/broadcast_rows.dart';
import 'package:meno_mobile/objectbox.g.dart';

/// What the broadcast screens keep on the device.
///
/// Two things, both scoped to the account they belong to: the drafts that
/// account is writing, and the broadcasts it keeps for later. The store answers
/// from its own cache, so a read is synchronous and a write is awaited only by
/// the caller that has something to do after it.
final class BroadcastLocalService {
  const new(this._database);

  final Database _database;

  /// Every draft [ownerId] has, newest edit first.
  List<BroadcastDraft> drafts(String ownerId) {
    return _database.draftBox
        .query(BroadcastDraftRow_.ownerId.equals(ownerId))
        .order(BroadcastDraftRow_.lastModified, flags: Order.descending)
        .build()
        .find()
        .map((row) => row.toDomain)
        .toList();
  }

  /// The draft with [draftId], or null.
  BroadcastDraft? draft(String draftId) {
    return _findDraft(draftId)?.toDomain;
  }

  /// Stores [draft], replacing the stored copy of the same draft.
  Future<void> saveDraft(BroadcastDraft draft) async {
    final stored = _findDraft(draft.id);
    final row = draft.copyWith(lastModified: DateTime.now()).toRow;
    if (stored != null) row.id = stored.id;
    await _database.draftBox.putAsync(row);
  }

  /// Forgets the draft with [draftId].
  Future<void> deleteDraft(String draftId) async {
    final stored = _findDraft(draftId);
    if (stored != null) await _database.draftBox.removeAsync(stored.id);
  }

  /// Whether [ownerId] keeps [broadcastId] for later.
  bool isFavourited({required String broadcastId, required String ownerId}) {
    return _query(
      broadcastId: broadcastId,
      ownerId: ownerId,
    ).build().findFirst() !=
        null;
  }

  /// Every broadcast [ownerId] keeps, most recently saved first.
  List<FavouriteBroadcast> favourites(String ownerId) {
    return _database.favouriteBroadcastBox
        .query(FavouriteBroadcastRow_.ownerId.equals(ownerId))
        .order(FavouriteBroadcastRow_.savedAt, flags: Order.descending)
        .build()
        .find()
        .map((row) => row.toDomain)
        .toList();
  }

  /// Keeps [broadcast], or forgets it when it was already kept.
  ///
  /// Returns whether it is kept afterwards, so the caller can settle the screen
  /// without asking again.
  Future<bool> toggleIsFavourite({
    required Broadcast broadcast,
    required String ownerId,
  }) async {
    final existing = _query(
      broadcastId: broadcast.id,
      ownerId: ownerId,
    ).build().findFirst();

    if (existing != null) {
      await _database.favouriteBroadcastBox.removeAsync(existing.id);
      return false;
    }

    await _database.favouriteBroadcastBox.putAsync(
      FavouriteBroadcast(
        broadcastId: broadcast.id,
        ownerId: ownerId,
        title: broadcast.title,
        creatorName: broadcast.hostName,
        imageUrl: broadcast.imageUrl,
        savedAt: DateTime.now(),
      ).toRow,
    );
    return true;
  }

  BroadcastDraftRow? _findDraft(String draftId) {
    return _database.draftBox
        .query(BroadcastDraftRow_.draftId.equals(draftId))
        .build()
        .findFirst();
  }

  QueryBuilder<FavouriteBroadcastRow> _query({
    required String broadcastId,
    required String ownerId,
  }) {
    return _database.favouriteBroadcastBox.query(
      FavouriteBroadcastRow_.broadcastId
          .equals(broadcastId)
          .and(FavouriteBroadcastRow_.ownerId.equals(ownerId)),
    );
  }
}
