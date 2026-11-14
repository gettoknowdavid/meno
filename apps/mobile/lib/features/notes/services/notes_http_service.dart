import 'package:dio/dio.dart' show CancelToken;
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/notes/model/_model.dart';

/// The notes and folders endpoints, one method each.
///
/// Paths and payloads are the ones the API serves — see
/// `apps/api/src/routes/notes.rs`. Two collections share the service because
/// they share a module: a note and the folder it is filed in are written by the
/// same handler on the server, and a note's folder is a `folderId` on its own
/// row rather than a second resource.
///
/// # Every write quotes a version
///
/// A note or folder carries the `version` the client last saw, and the API
/// refuses a write built on a stale one with a 409. That is what makes two
/// devices editing the same note a reported conflict instead of a silent
/// overwrite; the manager above turns that answer into the note the server
/// holds.
final class NotesHttpService {
  /// Creates the service over [_client].
  const new(this._client);

  final HttpClient _client;

  /// `GET /notes` — one page of notes, newest change first.
  ///
  /// [folderId] narrows to one folder, [pinned] to the pinned ones, and
  /// [keywords] matches the title and body through the API's search index.
  Future<CursorPage<Note>> notes({
    Id? folderId,
    bool? pinned,
    String? keywords,
    String? cursor,
    int limit = defaultLimit,
    CancelToken? cancelToken,
  }) {
    return _client.get(
      '/notes',
      queryParameters: {
        'folderId': ?folderId?.getOrNull,
        'pinned': ?pinned,
        'keywords': ?_term(keywords),
        'cursor': ?cursor,
        'limit': limit,
      },
      cancelToken: cancelToken,
      fromJson: (data) => CursorPage.fromJson(
        data,
        (item) => NoteDto.fromJson(item).toDomain,
        logger: _client.logger,
      ),
    );
  }

  /// `POST /notes` — creates a note under the client's own id.
  ///
  /// The id travels with the request so a note written offline keeps the one it
  /// was created with; a retried create is the same row, not a second note.
  Future<Note> createNote({
    required Id id,
    required String title,
    required String content,
    bool pinned = false,
    Id? folderId,
    CancelToken? cancelToken,
  }) {
    return _client.post(
      '/notes',
      data: {
        'id': id.getOrCrash,
        'title': title,
        'content': content,
        'pinned': pinned,
        'folderId': ?folderId?.getOrNull,
      },
      cancelToken: cancelToken,
      fromJson: (data) => NoteDto.fromJson(_map(data)).toDomain,
    );
  }

  /// `PATCH /notes/{id}` — writes the fields that changed.
  ///
  /// [folderId] is written unconditionally by the API, so a patch that omits it
  /// unfiles the note. The manager sends the note's whole current state, which
  /// is what makes that safe.
  Future<Note> updateNote(
    Id id, {
    required int baseVersion, String? title,
    String? content,
    bool? pinned,
    Id? folderId,
    CancelToken? cancelToken,
  }) {
    return _client.patch(
      '/notes/${id.getOrCrash}',
      data: {
        'title': ?title,
        'content': ?content,
        'pinned': ?pinned,
        'folderId': ?folderId?.getOrNull,
        'baseVersion': baseVersion,
      },
      cancelToken: cancelToken,
      fromJson: (data) => NoteDto.fromJson(_map(data)).toDomain,
    );
  }

  /// `DELETE /notes/{id}` — soft-deletes a note.
  Future<void> deleteNote(
    Id id, {
    required int baseVersion,
    CancelToken? cancelToken,
  }) {
    return _client.deleteUnit(
      '/notes/${id.getOrCrash}',
      data: {'baseVersion': baseVersion},
      cancelToken: cancelToken,
    );
  }

  /// `GET /folders` — one page of folders.
  Future<CursorPage<NoteFolder>> folders({
    bool? pinned,
    String? keywords,
    String? cursor,
    int limit = defaultLimit,
    CancelToken? cancelToken,
  }) {
    return _client.get(
      '/folders',
      queryParameters: {
        'pinned': ?pinned,
        'keywords': ?_term(keywords),
        'cursor': ?cursor,
        'limit': limit,
      },
      cancelToken: cancelToken,
      fromJson: (data) => CursorPage.fromJson(
        data,
        (item) => FolderDto.fromJson(item).toDomain,
        logger: _client.logger,
      ),
    );
  }

  /// `POST /folders` — creates a folder under the client's own id.
  Future<NoteFolder> createFolder({
    required Id id,
    required String title,
    bool pinned = false,
    CancelToken? cancelToken,
  }) {
    return _client.post(
      '/folders',
      data: {'id': id.getOrCrash, 'title': title, 'pinned': pinned},
      cancelToken: cancelToken,
      fromJson: (data) => FolderDto.fromJson(_map(data)).toDomain,
    );
  }

  /// `PATCH /folders/{id}` — renames or pins a folder.
  Future<NoteFolder> updateFolder(
    Id id, {
    required int baseVersion, String? title,
    bool? pinned,
    CancelToken? cancelToken,
  }) {
    return _client.patch(
      '/folders/${id.getOrCrash}',
      data: {'title': ?title, 'pinned': ?pinned, 'baseVersion': baseVersion},
      cancelToken: cancelToken,
      fromJson: (data) => FolderDto.fromJson(_map(data)).toDomain,
    );
  }

  /// `DELETE /folders/{id}` — deletes a folder, unfiling its notes.
  ///
  /// [shouldDeleteNotes] is the other branch: the API deletes the notes with
  /// the folder instead of unfiling them. It defaults to the safe one.
  Future<void> deleteFolder(
    Id id, {
    required int baseVersion,
    bool shouldDeleteNotes = false,
    CancelToken? cancelToken,
  }) {
    return _client.deleteUnit(
      '/folders/${id.getOrCrash}',
      data: {
        'baseVersion': baseVersion,
        'shouldDeleteNotes': shouldDeleteNotes,
      },
      cancelToken: cancelToken,
    );
  }

  /// `PUT /notes` — files a batch of notes into [folderId], or unfiles them
  /// when it is null.
  ///
  /// One request for the batch: the move is one intent, and the API applies it
  /// in one transaction, so a partial result is not a state this client has to
  /// repair.
  Future<void> moveNotes({
    required List<Id> noteIds,
    Id? folderId,
    CancelToken? cancelToken,
  }) {
    return _client.putUnit(
      '/notes',
      data: {
        'noteIds': [for (final id in noteIds) id.getOrCrash],
        'folderId': ?folderId?.getOrNull,
      },
      cancelToken: cancelToken,
    );
  }

  /// What the endpoint clamps a page to when a request does not say.
  static const int defaultLimit = 50;

  /// What a page may hold, at most.
  static const int maxLimit = 100;

  /// A keyword filter, or nothing when the bar is empty.
  static String? _term(String? keywords) {
    final trimmed = keywords?.trim();
    return (trimmed == null || trimmed.isEmpty) ? null : trimmed;
  }

  static Map<String, dynamic> _map(dynamic data) {
    if (data is Map<String, dynamic>) return data;
    throw const FormatException('Expected an object in the response.');
  }
}
