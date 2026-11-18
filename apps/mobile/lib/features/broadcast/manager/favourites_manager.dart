import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';
import 'package:meno_mobile/features/broadcast/services/_services.dart';

/// The broadcasts one account keeps for later.
///
/// The list is read from the device rather than the API: keeping a broadcast is
/// a local decision until it is not — the API's `is_bookmarked` has no endpoint
/// behind it yet — so the screen answers from the store it writes to.
///
/// Which account's list this is, is the store's answer, not this manager's: the
/// local service is created for one owner in the account scope, so nothing here
/// can ask for somebody else's favourites (audit M05).
final class FavouritesManager implements Disposable {
  new({
    required this._local,
    required this._logger,
    required Stream<BroadcastDeletedEvent> deletedBroadcasts,
  }) {
    // Built when the account is, not when the favourites tab is first opened:
    // a deletion can arrive while the reader is anywhere, and a local row that
    // is never purged keeps pointing at a broadcast the server has removed.
    _deleted = deletedBroadcasts.listen(_handleDeleted);

    fetch = .createSyncNoParamNoResult(
      () {
        final stored = _local.favourites();
        favourites.startTransAction();
        favourites.clear();
        favourites.addAll(stored);
        favourites.endTransAction();
        _logger.d(
          'Read ${stored.length} favourite broadcasts',
          tag: MenoLogTag.lifecycle.label,
        );
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'broadcast.favourites.fetch',
    );
    remove = .createAsyncNoResult<Id>(
      (broadcastId) async {
        await _local.forgetFavourite(broadcastId);
        fetch.run();
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'broadcast.favourites.remove',
    );

    // The list is the store's answer, so it is read as soon as this manager is
    // built: the favourites tab then paints what the account already kept
    // instead of starting empty and waiting for a write it may never see.
    fetch.run();

    // A row written anywhere else — the star on a details screen, the menu
    // sheet — is this same list, so the store says when it changed and the
    // copy held here is re-read.
    _changes = _local.favouriteChanges.listen((_) {
      if (!_isDisposed) fetch.run();
    });
  }

  final BroadcastLocalService _local;
  final MenoLogger _logger;

  late final StreamSubscription<BroadcastDeletedEvent> _deleted;
  late final StreamSubscription<void> _changes;
  bool _isDisposed = false;

  /// The account's kept broadcasts, most recently saved first.
  final favourites = ListNotifier<FavouriteBroadcast>(data: []);

  /// Re-reads the store.
  late final Command<void, void> fetch;

  /// Forgets `broadcastId`, then re-reads the list.
  late final Command<Id, void> remove;

  /// A deletion frame names only the broadcast; the local row is the record
  /// that this account kept it, so removing that row is the whole local side.
  void _handleDeleted(BroadcastDeletedEvent event) {
    unawaited(_forgetDeleted(event.broadcastId));
  }

  Future<void> _forgetDeleted(Id broadcastId) async {
    try {
      final removed = await _local.forgetFavourite(broadcastId);
      if (removed && !_isDisposed) fetch.run();
    } on Object catch (error, stackTrace) {
      _logger.w(
        'Could not forget a deleted favourite broadcast',
        tag: MenoLogTag.lifecycle.label,
        error: error,
        stackTrace: stackTrace,
      );
    }
  }

  @override
  FutureOr<dynamic> onDispose() async {
    _isDisposed = true;
    await _deleted.cancel();
    await _changes.cancel();

    favourites.dispose();
    fetch.dispose();
    remove.dispose();
  }
}
