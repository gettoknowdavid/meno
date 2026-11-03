import 'dart:async';

import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/notes/model/_model.dart';
import 'package:meno_mobile/features/notes/services/notes_http_service.dart';
import 'package:meno_mobile/features/notes/services/notes_local_service.dart';

/// Sends what the device has written and the server has not confirmed.
///
/// # Why it is separate from the managers
///
/// The screens write to the local store and stop there — that is what makes
/// typing a note instant and an offline edit possible. Getting those rows to
/// the API is a queue, and a queue has to be read back after a restart, retried
/// later, and drained in one place. Every manager that writes therefore calls
/// this service rather than the API, and [pushPending] is what runs when the
/// app comes back or a pull succeeds.
///
/// # Which write a row is
///
/// A row with no `createdAt` has never reached the server, so it is sent as a
/// create under the id it was typed with; a row that has one is a patch quoting
/// the version the server last wrote. That is decided here, from the row, so a
/// note that was created offline and edited twice is still one create.
///
/// # Failures are kept
///
/// A row that cannot be sent stays pending: the change is still on the device
/// and the next push tries again. A conflict — the server's version moved on
/// another device — is logged and left pending as well, so nothing typed here
/// is thrown away to make a request succeed.
final class NotesSyncService {
  /// Creates the queue over the API and the store it drains.
  new({
    required this._http,
    required this._local,
    required this._logger,
  });

  final NotesHttpService _http;
  final NotesLocalService _local;
  final MenoLogger _logger;

  /// How many rows are waiting, for a caller that wants to know.
  int get pendingCount =>
      _local.pendingNotes().length + _local.pendingFolders().length;

  /// Sends every row the server has not confirmed.
  ///
  /// One row failing does not stop the rest: they are independent changes, and
  /// a note whose upload was refused must not hold back the folder behind it.
  Future<void> pushPending() async {
    for (final note in _local.pendingNotes()) {
      try {
        if (note.deleted) {
          await _deleteNote(note);
        } else {
          await pushNote(note);
        }
      } on Object catch (error, stackTrace) {
        _logFailure('a note', note.id.getOrNull, error, stackTrace);
      }
    }

    for (final folder in _local.pendingFolders()) {
      try {
        if (folder.deleted) {
          await _deleteFolder(folder);
        } else {
          await pushFolder(folder);
        }
      } on Object catch (error, stackTrace) {
        _logFailure('a folder', folder.id.getOrNull, error, stackTrace);
      }
    }
  }

  /// Sends [note] and stores the server's answer.
  ///
  /// Throws when it could not be sent — the caller invoked this for one row and
  /// may want to say so; the row stays pending either way.
  Future<Note> pushNote(Note note) async {
    if (note.deleted) {
      await _deleteNote(note);
      return note;
    }

    final title = note.title.getOrElse((_) => '');
    final content = note.content.getOrElse((_) => '');

    final saved = note.createdAt == null
        ? await _http.createNote(
            id: note.id,
            title: title,
            content: content,
            pinned: note.pinned,
            folderId: note.folderId,
          )
        : await _http.updateNote(
            note.id,
            title: title,
            content: content,
            pinned: note.pinned,
            folderId: note.folderId,
            baseVersion: note.version,
          );

    await _local.saveNote(saved, pending: false);
    return saved;
  }

  /// Sends [folder] and stores the server's answer.
  Future<NoteFolder> pushFolder(NoteFolder folder) async {
    if (folder.deleted) {
      await _deleteFolder(folder);
      return folder;
    }

    final title = folder.title.getOrElse((_) => '');

    final saved = folder.createdAt == null
        ? await _http.createFolder(
            id: folder.id,
            title: title,
            pinned: folder.pinned,
          )
        : await _http.updateFolder(
            folder.id,
            title: title,
            pinned: folder.pinned,
            baseVersion: folder.version,
          );

    await _local.saveFolder(saved, pending: false);
    return saved;
  }

  /// Files [noteIds] into [folderId], or unfiles them when it is null.
  ///
  /// One request for the batch: the move is one intent, and the API applies it
  /// in one transaction. The rows are already filed locally, so a failure here
  /// leaves them pending rather than undoing what the user saw.
  Future<void> moveNotes(List<Id> noteIds, {Id? folderId}) async {
    await _http.moveNotes(noteIds: noteIds, folderId: folderId);
  }

  Future<void> _deleteNote(Note note) async {
    // A row the server never saw is deleted by forgetting it.
    if (note.createdAt != null) {
      await _http.deleteNote(note.id, baseVersion: note.version);
    }
    await _local.removeNote(note.id);
  }

  Future<void> _deleteFolder(NoteFolder folder) async {
    if (folder.createdAt != null) {
      await _http.deleteFolder(folder.id, baseVersion: folder.version);
    }
    await _local.removeFolder(folder.id);
  }

  void _logFailure(
    String what,
    String? id,
    Object error,
    StackTrace stackTrace,
  ) {
    _logger.w(
      'A pending $what ($id) could not be sent; it stays on the device',
      tag: MenoLogTag.network.label,
      error: error,
      stackTrace: stackTrace,
    );
  }
}
