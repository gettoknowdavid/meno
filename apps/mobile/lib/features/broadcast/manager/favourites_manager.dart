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
final class FavouritesManager implements Disposable {
  new({required this._local, required this._userId, required this._logger}) {
    fetch = .createSyncNoParamNoResult(
      () {
        final stored = _local.favourites(_userId);
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
        final stored = _local.favourites(_userId)
            .firstWhere((favourite) => favourite.broadcastId == broadcastId);
        await _local.toggleIsFavourite(
          broadcast: Broadcast(id: stored.broadcastId, title: stored.title),
          ownerId: _userId,
        );
        fetch.run();
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'broadcast.favourites.remove',
    );
  }

  final BroadcastLocalService _local;
  final Id _userId;
  final MenoLogger _logger;

  /// The account's kept broadcasts, most recently saved first.
  final favourites = ListNotifier<FavouriteBroadcast>(data: []);

  /// Re-reads the store.
  late final Command<void, void> fetch;

  /// Forgets `broadcastId`, then re-reads the list.
  late final Command<Id, void> remove;

  @override
  FutureOr<dynamic> onDispose() {
    favourites.dispose();
    fetch.dispose();
    remove.dispose();
  }
}
