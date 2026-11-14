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
///
/// # The owner is part of the service, not an argument
///
/// Isolation used to be a convention: every method took an `ownerId`, and every
/// caller had to pass the right one (audit M05). Here the owner is fixed when
/// the service is created — it lives in the account scope — so a caller cannot
/// name one, and a row belonging to somebody else is refused with
/// [core.OwnerViolationException] rather than returned or deleted. No `Box` is
/// handed out either: the store is the only thing that reads or writes these
/// entities.
///
/// # One row per logical entity
///
/// Each row carries a `uniqueKey` (`<ownerId>/<logicalId>`) with a unique
/// index that replaces on conflict, so an upsert is a single `put`: two
/// concurrent saves of the same draft cannot leave two rows (audit M06).
/// Writes are desired-state setters (`setFavourite(…, isFavourite: false)`),
/// never toggles, so replaying one cannot flip the value.
final class BroadcastLocalService {
  /// Creates the store for one account.
  ///
  /// An invalid owner is refused here instead of being flattened into rows that
  /// belong to nobody.
  new(this._ownerId, this._database, {required this._logger}) {
    if (!_ownerId.isValid) {
      throw const core.OwnerViolationException(
        message: 'A local store needs the account it belongs to.',
      );
    }
  }

  final core.Id _ownerId;
  final Database _database;
  final core.MenoLogger _logger;

  /// The owner every row of this service belongs to.
  core.Id get ownerId => _ownerId;

  /// Every draft this account has, newest edit first.
  List<BroadcastDraft> drafts() {
    final query = _database.draftBox
        .query(BroadcastDraftRow_.ownerId.equals(_ownerKey))
        .order(BroadcastDraftRow_.lastModified, flags: Order.descending)
        .build();
    try {
      return query.find().map((row) => row.toDomain(logger: _logger)).toList();
    } finally {
      query.close();
    }
  }

  /// The draft with [draftId] as this account knows it, or null.
  ///
  /// A row owned by another account is not a miss and not a result: it is an
  /// [core.OwnerViolationException] (M05), because silently returning null
  /// would hide that foreign data is reachable at all.
  BroadcastDraft? draft(core.Id draftId) {
    final row = _firstDraft(draftId.getOrCrash);
    if (row == null) return null;
    _requireOwner(row.ownerId);
    return row.toDomain(logger: _logger);
  }

  /// Stores [draft], replacing the row with the same logical key.
  ///
  /// A draft that names a different owner is foreign input and is refused. An
  /// existing row's owner is never rewritten — a draft that somehow became
  /// another account's is rejected rather than quietly re-owned.
  Future<void> saveDraft(BroadcastDraft draft) async {
    if (draft.ownerId.isValid && draft.ownerId != _ownerId) {
      throw const core.OwnerViolationException();
    }

    final stored = _firstDraft(draft.id.getOrCrash);
    if (stored != null) _requireOwner(stored.ownerId);

    // The unique index replaces the row holding the same `<owner>/<draft>` key,
    // so a concurrent save cannot create a second one.
    await _database.draftBox.putAsync(
      draft.copyWith(ownerId: _ownerId, lastModified: DateTime.now()).toRow,
    );
  }

  /// Forgets the draft with [draftId].
  ///
  /// A foreign row is rejected rather than deleted: "not mine" must never be
  /// able to destroy anything.
  Future<void> deleteDraft(core.Id draftId) async {
    final stored = _firstDraft(draftId.getOrCrash);
    if (stored == null) return;
    _requireOwner(stored.ownerId);
    await _database.draftBox.removeAsync(stored.id);
  }

  /// Whether this account keeps [broadcastId] for later.
  bool isFavourited(core.Id broadcastId) =>
      _firstFavourite(broadcastId.getOrCrash) != null;

  /// Every broadcast this account keeps, most recently saved first.
  List<FavouriteBroadcast> favourites() {
    final query = _database.favouriteBroadcastBox
        .query(FavouriteBroadcastRow_.ownerId.equals(_ownerKey))
        .order(FavouriteBroadcastRow_.savedAt, flags: Order.descending)
        .build();
    try {
      return query.find().map((row) => row.toDomain).toList();
    } finally {
      query.close();
    }
  }

  /// Keeps [broadcast] when [isFavourite], forgets it otherwise.
  ///
  /// A desired state, not a toggle: calling it twice with the same value leaves
  /// one row kept (or none), which is what makes a repeated tap harmless.
  Future<void> setFavourite(
    Broadcast broadcast, {
    required bool isFavourite,
  }) async {
    final broadcastId = broadcast.id.getOrCrash;

    if (!isFavourite) {
      final stored = _firstFavourite(broadcastId);
      if (stored != null) {
        await _database.favouriteBroadcastBox.removeAsync(stored.id);
      }
      return;
    }

    await _database.favouriteBroadcastBox.putAsync(
      FavouriteBroadcast(
        broadcastId: broadcast.id,
        ownerId: _ownerId,
        title: broadcast.title,
        creatorName: broadcast.hostName,
        image: broadcast.image,
        savedAt: DateTime.now(),
      ).toRow,
    );
  }

  /// The account id as every row stores it.
  String get _ownerKey => _ownerId.getOrCrash;

  /// Removes duplicate rows an older build left for this account.
  ///
  /// The unique index prevents new duplicates, but a database written before
  /// it existed can already hold two rows for one draft (audit M06, assumption
  /// A13: installed builds may exist). Newest wins, ties broken by the higher
  /// native row id, so the surviving row is the same one on every run. Nothing
  /// is merged: a duplicate is a stale copy, not half a draft.
  Future<void> dedupe() async {
    final drafts = _database.draftBox
        .query(BroadcastDraftRow_.ownerId.equals(_ownerKey))
        .build();
    try {
      await _dropOlder<BroadcastDraftRow>(
        rows: drafts.find(),
        key: (row) => row.uniqueKey,
        at: (row) => row.lastModified,
        id: (row) => row.id,
        remove: _database.draftBox.removeAsync,
      );
    } finally {
      drafts.close();
    }

    final favourites = _database.favouriteBroadcastBox
        .query(FavouriteBroadcastRow_.ownerId.equals(_ownerKey))
        .build();
    try {
      await _dropOlder<FavouriteBroadcastRow>(
        rows: favourites.find(),
        key: (row) => row.uniqueKey,
        at: (row) => row.savedAt,
        id: (row) => row.id,
        remove: _database.favouriteBroadcastBox.removeAsync,
      );
    } finally {
      favourites.close();
    }
  }

  /// Keeps the newest row per key and removes the rest.
  Future<void> _dropOlder<T>({
    required List<T> rows,
    required String Function(T row) key,
    required DateTime Function(T row) at,
    required int Function(T row) id,
    required Future<void> Function(int id) remove,
  }) async {
    final survivors = <String, T>{};
    final stale = <T>[];

    for (final row in rows) {
      final existing = survivors[key(row)];
      if (existing == null) {
        survivors[key(row)] = row;
        continue;
      }

      final newer =
          at(row).isAfter(at(existing)) ||
          (at(row) == at(existing) && id(row) > id(existing));
      if (newer) {
        stale.add(existing);
        survivors[key(row)] = row;
      } else {
        stale.add(row);
      }
    }

    for (final row in stale) {
      await remove(id(row));
    }
  }

  BroadcastDraftRow? _firstDraft(String draftId) {
    final query = _database.draftBox
        .query(BroadcastDraftRow_.draftId.equals(draftId))
        .build();
    try {
      return query.findFirst();
    } finally {
      query.close();
    }
  }

  FavouriteBroadcastRow? _firstFavourite(String broadcastId) {
    final query = _database.favouriteBroadcastBox
        .query(FavouriteBroadcastRow_.broadcastId.equals(broadcastId))
        .build();
    try {
      return query.findFirst();
    } finally {
      query.close();
    }
  }

  /// Throws unless [storedOwnerId] is this service's owner.
  void _requireOwner(String storedOwnerId) {
    if (storedOwnerId == _ownerKey) return;
    _logger.w(
      'Refused a row owned by another account',
      tag: core.MenoLogTag.di.label,
    );
    throw const core.OwnerViolationException();
  }
}
