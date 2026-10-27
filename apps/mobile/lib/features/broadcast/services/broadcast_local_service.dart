import 'package:meno_mobile/_core/_core.dart' as core;
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
  const new(this._database, {required this._logger});

  final Database _database;
  final core.MenoLogger _logger;

  /// Every draft [ownerId] has, newest edit first.
  List<BroadcastDraft> drafts(core.Id ownerId) {
    return _database.draftBox
        .query(BroadcastDraftRow_.ownerId.equals(ownerId.getOrCrash))
        .order(BroadcastDraftRow_.lastModified, flags: Order.descending)
        .build()
        .find()
        .map((row) => row.toDomain(logger: _logger))
        .toList();
  }

  /// The draft with [draftId] as [ownerId] knows it, or null.
  ///
  /// A row owned by another account is not a miss and not a result: it is an
  /// [core.OwnerViolationException] (M05), because silently returning null
  /// would hide that foreign data is reachable at all.
  BroadcastDraft? draft({required core.Id draftId, required core.Id ownerId}) {
    final row = _findDraft(draftId.getOrCrash);
    if (row == null) return null;
    _requireOwner(row.ownerId, ownerId);
    return row.toDomain(logger: _logger);
  }

  /// Stores [draft], replacing the stored copy of the same draft.
  ///
  /// [ownerId] is required: the write either lands under the caller's own id or
  /// is refused. An existing row's owner is never rewritten — a draft that
  /// somehow became another account's is rejected rather than quietly
  /// re-owned.
  Future<void> saveDraft(
    BroadcastDraft draft, {
    required core.Id ownerId,
  }) async {
    // A draft that already names a different owner is foreign input.
    if (draft.ownerId.isValid && draft.ownerId != ownerId) {
      throw const core.OwnerViolationException();
    }

    final stored = _findDraft(draft.id.getOrCrash);
    if (stored != null) _requireOwner(stored.ownerId, ownerId);

    final row = draft
        .copyWith(ownerId: ownerId, lastModified: DateTime.now())
        .toRow;
    if (stored != null) row.id = stored.id;
    await _database.draftBox.putAsync(row);
  }

  /// Forgets the draft with [draftId].
  ///
  /// A foreign row is rejected rather than deleted: "not mine" must never be
  /// able to destroy anything.
  Future<void> deleteDraft({
    required core.Id draftId,
    required core.Id ownerId,
  }) async {
    final stored = _findDraft(draftId.getOrCrash);
    if (stored == null) return;
    _requireOwner(stored.ownerId, ownerId);
    await _database.draftBox.removeAsync(stored.id);
  }

  /// Whether [ownerId] keeps [broadcastId] for later.
  bool isFavourited({required core.Id broadcastId, required core.Id ownerId}) {
    return _query(
          broadcastId: broadcastId.getOrCrash,
          ownerId: ownerId.getOrCrash,
        ).build().findFirst() !=
        null;
  }

  /// Every broadcast [ownerId] keeps, most recently saved first.
  List<FavouriteBroadcast> favourites(core.Id ownerId) {
    return _database.favouriteBroadcastBox
        .query(FavouriteBroadcastRow_.ownerId.equals(ownerId.getOrCrash))
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
    required core.Id ownerId,
  }) async {
    final existing = _query(
      broadcastId: broadcast.id.getOrCrash,
      ownerId: ownerId.getOrCrash,
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
        image: broadcast.image,
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

  /// Throws unless [storedOwnerId] is [requestedOwner].
  void _requireOwner(String storedOwnerId, core.Id requestedOwner) {
    if (storedOwnerId == requestedOwner.getOrCrash) return;
    _logger.w(
      'Refused a draft owned by another account',
      tag: core.MenoLogTag.di.label,
    );
    throw const core.OwnerViolationException();
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
