import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/notification/manager/notifications_manager.dart';
import 'package:meno_mobile/features/notification/model/entities.dart';
import 'package:meno_mobile/features/notification/services/notifications_http_service.dart';

/// One notification as a card draws it, with the reader's own edit layered on.
///
/// A card needs the stored row and one local decision — whether it draws as
/// read — so it watches this rather than the feed. Marking read is optimistic:
/// the card stops drawing the unread dot immediately and the write follows,
/// with the override restored if the write fails, because a card that waits
/// for the network before losing its dot feels broken.
final class NotificationProxy extends ChangeNotifier {
  /// Wraps [notification].
  new(this._notification);

  Notification _notification;

  /// The read decision the reader made, while it is newer than the stored row.
  bool? _isReadOverride;

  /// Marks the notification read.
  late final Command<void, void> markAsRead =
      .createUndoableNoParamNoResult<bool>(
        (stack) async {
          final id = _notification.id;
          if (id == null) return;

          stack.push(isRead);
          // Marking read is the postcondition, not a toggle: there is no
          // endpoint that puts a notification back to unread.
          _isReadOverride = true;
          _syncBadge();
          notifyListeners();

          await di<NotificationsHttpService>().markAsRead(id);
        },
        undo: (stack, _) {
          _isReadOverride = stack.pop();
          _syncBadge();
          notifyListeners();
        },
        errorFilter: null,
        errorFilterFn: menoExceptionFilter,
        debugName: 'notification.markAsRead',
      );

  /// Removes the notification from the inbox.
  late final Command<void, void> delete =
      .createUndoableNoParamNoResult<Notification>(
        (stack) async {
          final id = _notification.id;
          if (id == null) return;

          stack.push(_notification);
          await di<NotificationsHttpService>().deleteNotification(id);
        },
        undo: (stack, _) => stack.pop(),
        errorFilter: null,
        errorFilterFn: menoExceptionFilter,
        debugName: 'notification.delete',
      );

  /// The stored notification.
  Notification get notification => _notification;

  /// Whether the card draws as read: the override if the reader acted, else the
  /// stored value.
  bool get isRead => _isReadOverride ?? _notification.read;

  /// Replaces the stored row and drops the override.
  ///
  /// Fresh data is newer than any local decision, so the override the reader
  /// set has done its job by the time a refresh lands.
  set notification(Notification value) {
    _isReadOverride = null;
    _notification = value;
    notifyListeners();
  }

  /// Tells the badge to re-read the count, which this card just changed.
  void _syncBadge() {
    if (!di.isRegistered<NotificationsManager>()) return;
    unawaited(di<NotificationsManager>().syncUnreadCount());
  }

  @override
  void dispose() {
    markAsRead.dispose();
    delete.dispose();
    super.dispose();
  }
}
