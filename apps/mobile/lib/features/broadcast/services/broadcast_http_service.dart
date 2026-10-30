import 'dart:io' show File;

import 'package:dio/dio.dart' show CancelToken, FormData, MultipartFile;
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';

/// The broadcast endpoints, one method each.
///
/// Paths and payloads are the ones the API serves — see
/// `apps/api/src/routes/broadcasts.rs`. Every write is a JSON body except the
/// cover, which is a multipart upload: an image is stored through
/// [uploadImage], and a caller that already has its key sends it as
/// `image_id`/`image_url` instead.
final class BroadcastHttpService {
  const new(this._client);

  final HttpClient _client;

  /// `GET /broadcasts` — one page of broadcasts matching [query].
  Future<CursorPage<BroadcastSummary>> list(
    BroadcastQuery query, {
    CancelToken? cancelToken,
  }) {
    return _client.get(
      '/broadcasts',
      queryParameters: query.toQueryParameters(),
      cancelToken: cancelToken,
      fromJson: (data) => CursorPage.fromJson(
        data,
        (item) => BroadcastListItemDto.fromJson(item).toDomain,
        logger: _client.logger,
      ),
    );
  }

  /// `GET /broadcasts/{id}` — one broadcast, with the viewer's standing.
  Future<Broadcast> getBroadcast(Id broadcastId, {CancelToken? cancelToken}) {
    return _client.get(
      '/broadcasts/${broadcastId.getOrCrash}',
      cancelToken: cancelToken,
      fromJson: (data) => BroadcastDto.fromJson(_map(data)).toDomain,
    );
  }

  /// `GET /broadcasts/{id}/participants` — one page of the room's accounts.
  Future<CursorPage<Participant>> participants(
    Id broadcastId, {
    ParticipantRole? role,
    BroadcastOrder? order,
    String? cursor,
    int limit = 50,
    CancelToken? cancelToken,
  }) {
    return _client.get(
      '/broadcasts/${broadcastId.getOrCrash}/participants',
      queryParameters: {
        'role': ?role?.wire,
        'order': ?order?.wire,
        'cursor': ?cursor,
        'limit': limit,
      },
      cancelToken: cancelToken,
      fromJson: (data) => CursorPage.fromJson(
        data,
        (item) => ParticipantDto.fromJson(item).toDomain,
        logger: _client.logger,
      ),
    );
  }

  /// `POST /broadcasts` — creates a draft, a scheduled broadcast or a live one.
  ///
  /// [idempotencyKey] belongs to the *draft*, not to the call: it is the same
  /// on every attempt at creating the row this draft describes, including the
  /// one the user makes after an ambiguous timeout. A replay is answered from
  /// the API's stored outcome instead of creating a second broadcast.
  Future<Broadcast> create({
    required String title,
    required String idempotencyKey,
    String? description,
    String? imageId,
    String? imageUrl,
    String? timeZone,
    DateTime? startTime,
    bool? recordingEnabled,
    List<String>? cohosts,
    CancelToken? cancelToken,
  }) {
    return _client.post(
      '/broadcasts',
      data: {
        'title': title,
        'description': ?description,
        'image_id': ?imageId,
        'image_url': ?imageUrl,
        'time_zone': ?timeZone,
        'start_time': ?startTime?.toUtc().toIso8601String(),
        'recording_enabled': ?recordingEnabled,
        if (cohosts != null && cohosts.isNotEmpty) 'cohosts': cohosts,
      },
      cancelToken: cancelToken,
      idempotencyKey: idempotencyKey,
      fromJson: (data) => BroadcastDto.fromJson(_map(data)).toDomain,
    );
  }

  /// `POST /broadcasts/{id}/image` — stores [file] as the broadcast's cover.
  ///
  /// The only way a cover gets a URL: the bytes pass through the API, which
  /// sniffs them, writes the object and answers with the record carrying the
  /// new `imageUrl`. It has to happen while the row is still a draft — the API
  /// refuses a cover change on a broadcast that is on air — so the editor calls
  /// it between creating the row and going live.
  Future<Broadcast> uploadImage(
    Id broadcastId,
    File file, {
    CancelToken? cancelToken,
  }) {
    return _client.upload(
      '/broadcasts/${broadcastId.getOrCrash}/image',
      formData: FormData()
        ..files.add(
          MapEntry(
            'file',
            MultipartFile.fromFileSync(
              file.path,
              filename: file.uri.pathSegments.last,
            ),
          ),
        ),
      cancelToken: cancelToken,
      fromJson: (data) => BroadcastDto.fromJson(_map(data)).toDomain,
    );
  }

  /// `PATCH /broadcasts/{id}` — a partial update, answering with the record.
  Future<Broadcast> update(
    Id broadcastId, {
    String? title,
    String? description,
    String? imageId,
    String? imageUrl,
    String? timeZone,
    DateTime? startTime,
    bool? recordingEnabled,
    List<String>? cohosts,
    CancelToken? cancelToken,
  }) {
    return _client.patch(
      '/broadcasts/${broadcastId.getOrCrash}',
      data: {
        'title': ?title,
        'description': ?description,
        'image_id': ?imageId,
        'image_url': ?imageUrl,
        'time_zone': ?timeZone,
        'start_time': ?startTime?.toUtc().toIso8601String(),
        'recording_enabled': ?recordingEnabled,
        'cohosts': ?cohosts,
      },
      cancelToken: cancelToken,
      fromJson: (data) => BroadcastDto.fromJson(_map(data)).toDomain,
    );
  }

  /// `DELETE /broadcasts/{id}`.
  Future<void> remove(Id broadcastId, {CancelToken? cancelToken}) {
    return _client.deleteUnit(
      '/broadcasts/${broadcastId.getOrCrash}',
      cancelToken: cancelToken,
    );
  }

  /// `POST /broadcasts/{id}/live` — goes on air, answering with the token.
  Future<BroadcastSessionDto> goLive(
    Id broadcastId, {
    CancelToken? cancelToken,
    String? idempotencyKey,
  }) {
    return _client.post(
      '/broadcasts/${broadcastId.getOrCrash}/live',
      cancelToken: cancelToken,
      idempotencyKey: idempotencyKey,
      fromJson: (data) => BroadcastSessionDto.fromJson(_map(data)),
    );
  }

  /// `POST /broadcasts/{id}/end` — takes the broadcast off air.
  Future<EndedBroadcast> end(
    Id broadcastId, {
    CancelToken? cancelToken,
    String? idempotencyKey,
  }) {
    return _client.post(
      '/broadcasts/${broadcastId.getOrCrash}/end',
      cancelToken: cancelToken,
      idempotencyKey: idempotencyKey,
      fromJson: (data) => EndBroadcastDto.fromJson(_map(data)).toDomain,
    );
  }

  /// `POST /broadcasts/{id}/join` — subscribes, answering with the token.
  Future<BroadcastSessionDto> join(
    Id broadcastId, {
    CancelToken? cancelToken,
    String? idempotencyKey,
  }) {
    return _client.post(
      '/broadcasts/${broadcastId.getOrCrash}/join',
      cancelToken: cancelToken,
      idempotencyKey: idempotencyKey,
      fromJson: (data) => BroadcastSessionDto.fromJson(_map(data)),
    );
  }

  /// `POST /broadcasts/{id}/resume` — comes back into a broadcast on air.
  ///
  /// What a launch calls when the previous process died inside a room. Neither
  /// of the other two works from there: [goLive] refuses a broadcast that is
  /// already live, and [join] refuses the host. The token in the stored
  /// session is no use either — the API signs one for minutes, so by the time
  /// the app is back it has expired — which is why coming back asks for a new
  /// one.
  Future<BroadcastSessionDto> resume(
    Id broadcastId, {
    CancelToken? cancelToken,
  }) {
    return _client.post(
      '/broadcasts/${broadcastId.getOrCrash}/resume',
      cancelToken: cancelToken,
      fromJson: (data) => BroadcastSessionDto.fromJson(_map(data)),
    );
  }

  /// `POST /broadcasts/{id}/leave` — stops listening.
  Future<LeaveBroadcastDto> leave(Id broadcastId, {CancelToken? cancelToken}) {
    return _client.post(
      '/broadcasts/${broadcastId.getOrCrash}/leave',
      cancelToken: cancelToken,
      fromJson: (data) => LeaveBroadcastDto.fromJson(_map(data)),
    );
  }

  /// `POST /broadcasts/{id}/cohosts` — invites [cohostId] to publish.
  Future<CohostDto> addCohost(
    Id broadcastId,
    Id cohostId, {
    CancelToken? cancelToken,
  }) {
    return _client.post(
      '/broadcasts/${broadcastId.getOrCrash}/cohosts',
      data: {'cohost': cohostId.getOrCrash},
      cancelToken: cancelToken,
      fromJson: (data) => CohostDto.fromJson(_map(data)),
    );
  }

  /// `DELETE /broadcasts/{id}/cohosts/{cohostId}`.
  ///
  /// [removeFromRoom] also drops them from the LiveKit room; without it they
  /// keep listening and only lose the ability to publish.
  Future<CohostDto> removeCohost(
    Id broadcastId,
    Id cohostId, {
    bool removeFromRoom = true,
    CancelToken? cancelToken,
  }) {
    return _client.delete(
      '/broadcasts/${broadcastId.getOrCrash}/cohosts/${cohostId.getOrCrash}',
      queryParameters: {'remove_from_room': removeFromRoom},
      cancelToken: cancelToken,
      fromJson: (data) => CohostDto.fromJson(_map(data)),
    );
  }

  static Map<String, dynamic> _map(dynamic data) {
    if (data is! Map<String, dynamic>) {
      throw MalformedResponseException(
        cause: data,
        message: 'Expected a broadcast object but got ${data.runtimeType}.',
      );
    }
    return data;
  }
}
