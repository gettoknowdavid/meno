import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';
import 'package:meno_mobile/features/broadcast/services/_services.dart';

/// The broadcasts one account keeps for later.
///
/// The API is the record: a bookmark belongs to the account, so the page
/// `GET /broadcasts/bookmarks` answers with replaces the device copy and the
/// list the tab draws. The device copy stays because a star has to be able to
/// say "kept" on the frame a screen opens, before any request has landed.
///
/// Which account's list this is, is the local store's answer: it is created for
/// one owner in the account scope, so nothing here can ask for somebody else's
/// favourites (audit M05).
final class FavouritesManager implements Disposable {
  new({
    required this._local,
    required this._http,
    required this._logger,
    required Stream<BroadcastDeletedEvent> deletedBroadcasts,
  }) {
    // Built when the account is, not when the favourites tab is first opened:
    // a deletion can arrive while the reader is anywhere, and a local row that
    // is never purged keeps pointing at a broadcast the server has removed.
    _deleted = deletedBroadcasts.listen(_handleDeleted);

    fetch = .createAsyncNoParamNoResult(
      () async {
        // The account's list, not this device's: whatever another phone kept
        // has to appear here, and whatever this one kept that the server does
        // not have is not a favourite any more.
        // One page, not the whole history: the tab is a list somebody scrolls,
        // and the API answers a page at a time.
        final page = await _http.bookmarks();
        final stored = await _local.replaceFavourites([
          for (final bookmark in page.items)
            FavouriteBroadcast(
              broadcastId: bookmark.summary.id,
              ownerId: _local.ownerId,
              title: bookmark.summary.title,
              creatorName: bookmark.summary.creatorName,
              image: bookmark.summary.image,
              savedAt: bookmark.savedAt,
            ),
        ]);

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
        await _http.removeBookmark(broadcastId);
        await _local.forgetFavourite(broadcastId);
        fetch.run();
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'broadcast.favourites.remove',
    );

    // The device copy is read as soon as this manager is built, so the tab
    // paints what the account kept last time instead of starting empty while
    // the first request is on its way.
    fetch.run();

    // A row written anywhere else — the star on a details screen — is this same
    // list, so the store says when it changed and the page is read again. The
    // writes that reach here are the ones the API already accepted, which is
    // what keeps this read from overtaking them.
    _changes = _local.favouriteChanges.listen((_) {
      if (!_isDisposed) fetch.run();
    });
  }

  final BroadcastLocalService _local;
  final BroadcastHttpService _http;
  final MenoLogger _logger;

  late final StreamSubscription<BroadcastDeletedEvent> _deleted;
  late final StreamSubscription<void> _changes;
  bool _isDisposed = false;

  /// The account's kept broadcasts, most recently saved first.
  final favourites = ListNotifier<FavouriteBroadcast>(data: []);

  /// Reads the account's bookmarks and refreshes the device copy.
  late final Command<void, void> fetch;

  /// Forgets `broadcastId` on the server and the device, then re-reads the
  /// list.
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
