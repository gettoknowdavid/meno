import 'package:dio/dio.dart' show CancelToken;
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/chat/model/_model.dart';

/// The one read a chat has.
///
/// Everything that writes chat arrives on the socket — `apps/api/src/routes/chat.rs`
/// serves exactly `GET /broadcasts/{id}/chat/messages` and nothing else — so a
/// feed depends on a page reader, not on a client, and a test can answer pages
/// without a transport.
abstract interface class ChatMessagePageReader {
  /// Reads one page of [broadcastId]'s chat history, newest first.
  ///
  /// [cursor] is the opaque cursor the previous page handed back,
  /// or null to ask for the first page.
  Future<CursorPage<Message>> messages(
    Id broadcastId, {
    String? cursor,
    int limit,
    CancelToken? cancelToken,
  });
}

/// The chat module's HTTP surface.
///
/// The endpoint refuses a caller who is not in the room (`403 NOT_PARTICIPANT`)
/// and answers with the API's cursor envelope:
///
/// ```json
/// { "data": [...], "next_cursor": "opaque", "has_next_page": true }
/// ```
///
/// The cursor is stored and handed back untouched — this client never
/// builds one.
final class ChatHttpService implements ChatMessagePageReader {
  const new(this._client);

  final HttpClient _client;

  /// How many messages one page holds when the caller does not say.
  static const int defaultPageSize = 50;

  @override
  Future<CursorPage<Message>> messages(
    Id broadcastId, {
    String? cursor,
    int limit = defaultPageSize,
    CancelToken? cancelToken,
  }) {
    return _client.get(
      '/broadcasts/${broadcastId.getOrCrash}/chat/messages',
      queryParameters: {'cursor': ?cursor, 'limit': limit},
      cancelToken: cancelToken,
      fromJson: (data) => CursorPage.fromJson(
        data,
        (item) => MessageDto.fromJson(item).toDomain,
        logger: _client.logger,
      ),
    );
  }
}
