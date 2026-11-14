import 'package:dio/dio.dart' show CancelToken;
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/notification/model/_model.dart';

/// The notifications endpoints, one method each.
///
/// Mirrors `apps/api/src/routes/notifications.rs`: a keyset-paged inbox, the
/// cheap unread count behind the badge, a per-row mark-read, a mark-all, and a
/// delete that answers `204` whether or not the row was there.
///
/// Nothing here caches or decides: the manager owns the badge, the feed owns
/// pagination, and the proxy owns the optimistic "read" the card draws.
final class NotificationsHttpService {
  /// Creates a service over [_client].
  const new(this._client);

  final HttpClient _client;

  /// How many notifications one page holds.
  static const int defaultLimit = 20;

  /// `GET /notifications` — one page of this account's inbox.
  ///
  /// [unreadOnly] narrows to the unread rows, [cursor] names the page and is
  /// opaque — `null` asks for the first one.
  Future<CursorPage<Notification>> getNotifications({
    bool unreadOnly = false,
    String? cursor,
    int limit = defaultLimit,
    CancelToken? cancelToken,
  }) {
    return _client.get(
      '/notifications',
      queryParameters: {
        if (unreadOnly) 'unreadOnly': true,
        'cursor': ?cursor,
        'limit': limit,
      },
      cancelToken: cancelToken,
      fromJson: (data) => CursorPage.fromJson(
        data,
        (item) => NotificationDto.fromJson(item).toDomain,
        logger: _client.logger,
      ),
    );
  }

  /// `GET /notifications/unread-count` — the number behind the badge.
  ///
  /// Read from the API's counter rather than counted from the loaded page: the
  /// inbox pages, so the rows this client holds are not the whole story.
  Future<int> unreadCount({CancelToken? cancelToken}) {
    return _client.get(
      '/notifications/unread-count',
      cancelToken: cancelToken,
      fromJson: _unreadCount,
    );
  }

  /// `PATCH /notifications/{id}/read` — mark one notification read.
  ///
  /// The API answers with the row's id and `read: true`; the postcondition is
  /// all the caller needs, so the answer is not returned.
  Future<void> markAsRead(String id, {CancelToken? cancelToken}) {
    return _client.patch(
      '/notifications/$id/read',
      cancelToken: cancelToken,
      fromJson: (_) {},
    );
  }

  /// `PATCH /notifications/read-all` — mark the whole inbox read.
  ///
  /// Answers how many rows changed, which is what a "marked N as read" line
  /// would say.
  Future<int> markAllAsRead({CancelToken? cancelToken}) {
    return _client.patch(
      '/notifications/read-all',
      cancelToken: cancelToken,
      fromJson: _updatedCount,
    );
  }

  /// `DELETE /notifications/{id}` — remove one notification.
  Future<void> deleteNotification(String id, {CancelToken? cancelToken}) {
    return _client.deleteUnit('/notifications/$id', cancelToken: cancelToken);
  }

  /// Reads `{count}`.
  static int _unreadCount(dynamic data) {
    if (data is! Map<String, dynamic>) {
      throw const FormatException(
        'Expected an unread-count object but got a different value.',
      );
    }
    return (data['count'] as num?)?.toInt() ?? 0;
  }

  /// Reads `{updated}`.
  static int _updatedCount(dynamic data) {
    if (data is! Map<String, dynamic>) {
      throw const FormatException(
        'Expected a mark-all-read object but got a different value.',
      );
    }
    return (data['updated'] as num?)?.toInt() ?? 0;
  }
}
