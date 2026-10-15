import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/model/entities.dart';
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
    _isFavourited = _storage.isFavourited(
      broadcastId: _broadcast.id,
      ownerId: _ownerId,
    );

    toggleIsFavourite = .createUndoableNoParamNoResult<bool>(
      (stack) async {
        stack.push(_isFavourited);
        _isFavourited = !_isFavourited;
        notifyListeners();
        await _storage.toggleIsFavourite(
          broadcast: _broadcast,
          ownerId: _ownerId,
        );
      },
      undo: (stack, _) {
        _isFavourited = stack.pop();
        notifyListeners();
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'broadcast.toggleIsFavourite',
    );
  }

  final String _ownerId;
  Broadcast _broadcast;

  late bool _isFavourited;

  /// Keeps the broadcast, or forgets it, undoing the flag when the write fails.
  late final Command<void, void> toggleIsFavourite;

  BroadcastLocalService get _storage => di<BroadcastLocalService>();

  Broadcast get broadcast => _broadcast;

  bool get isFavourited => _isFavourited;

  String get id => _broadcast.id;

  String get ownerId => _ownerId;

  String get title => _broadcast.title;

  String get description => _broadcast.description ?? '';

  String get creatorName => _broadcast.hostName;

  String? get imageUrl => _broadcast.imageUrl;

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
