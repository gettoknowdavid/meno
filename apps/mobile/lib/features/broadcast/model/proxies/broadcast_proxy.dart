import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/model/entities.dart';
import 'package:meno_mobile/features/broadcast/services/broadcast_http_service.dart';
import 'package:meno_mobile/features/broadcast/services/broadcast_local_service.dart';

/// One broadcast as a card draws it, with the viewer's own edits layered on.
///
/// A card needs a few fields and one local decision — whether the broadcast is
/// kept — so it watches this rather than the whole entity. Keeping it is
/// optimistic: the flag flips immediately and the write follows, with the flip
/// undone if the write fails, because a bookmark that waits for the disk to
/// answer feels broken.
final class BroadcastProxy extends ChangeNotifier {
  new(this._broadcast, this._ownerId) {
    // The server's answer wins when it says kept, the device copy otherwise:
    // a bookmark made on another phone is a fact this one has not written yet,
    // and a card drawn from a detail response must not show the wrong star.
    _isFavourited =
        _broadcast.isBookmarked || _storage.isFavourited(_broadcast.id);

    toggleIsFavourite = .createUndoableNoParamNoResult<bool>(
      (stack) async {
        stack.push(_isFavourited);
        _isFavourited = !_isFavourited;
        // The flip above is the whole of the optimism: the star fills on the
        // same frame as the tap, while the writes below take as long as they
        // take, and a failure puts the old flag back.
        notifyListeners();

        // The API goes first so the device row is only written for a state the
        // server accepted. The favourites tab reads the server, and a row
        // written ahead of it would be reverted by the next read.
        if (_isFavourited) {
          await _http.addBookmark(_broadcast.id);
        } else {
          await _http.removeBookmark(_broadcast.id);
        }

        // The desired state that was just shown, not a toggle: if the write is
        // retried or the disk already holds it, the result is the same row.
        await _storage.setFavourite(_broadcast, isFavourite: _isFavourited);
      },
      undo: (stack, _) async {
        final previous = stack.pop();
        _isFavourited = previous;
        notifyListeners();
        // The device copy is put back too: the request that failed never
        // reached the API, and an undone star has to leave the list agreeing
        // with it.
        await _storage.setFavourite(_broadcast, isFavourite: previous);
      },
      errorFilter: null,
      errorFilterFn: menoExceptionFilter,
      debugName: 'broadcast.toggleIsFavourite',
    );
  }

  final Id _ownerId;
  Broadcast _broadcast;

  late bool _isFavourited;

  /// Keeps the broadcast, or forgets it, undoing the flag when the write fails.
  late final Command<void, void> toggleIsFavourite;

  BroadcastLocalService get _storage => di<BroadcastLocalService>();

  BroadcastHttpService get _http => di<BroadcastHttpService>();

  Broadcast get broadcast => _broadcast;

  bool get isFavourited => _isFavourited;

  Id get id => _broadcast.id;

  Id get ownerId => _ownerId;

  /// The account that owns the broadcast, which is not the same as [id].
  Id get hostId => _broadcast.hostId;

  SingleLineString get title => _broadcast.title;

  MultiLineString get description => _broadcast.description ?? .empty;

  SingleLineString get creatorName => _broadcast.hostName;

  ImageInput? get image => _broadcast.image;

  bool get isLive => _broadcast.isLive;

  bool get isHostedByMe => _broadcast.isHostedBy(_ownerId);

  set broadcast(Broadcast value) {
    if (_broadcast == value) return;
    _broadcast = value;
    notifyListeners();
  }

  @override
  void dispose() {
    toggleIsFavourite.dispose();
    super.dispose();
  }
}
