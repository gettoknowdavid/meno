import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/notes/manager/folders_manager.dart';
import 'package:meno_mobile/features/notes/manager/notes_manager.dart';
import 'package:meno_mobile/features/notes/model/_model.dart';
import 'package:meno_mobile/features/notes/services/_services.dart';

/// What can be done to a note or a folder from a card or a menu.
///
/// # Written here first
///
/// Every action lands in the local store before it is sent: the row changes,
/// the list the user is looking at changes with it, and the request follows. A
/// tap therefore does not depend on the network, and a request that fails
/// leaves a pending row rather than an action that silently did nothing.
///
/// # Why the commands take whole rows
///
/// A write quotes the version the caller last saw, so an action takes the note
/// or folder the list drew rather than its id. An id would force this manager
/// to look the row up again and hope nothing had changed since, and a stale
/// version is exactly the write the API refuses.
final class NoteActionsManager implements Disposable {
  /// Creates the manager over the store and the queue that drains it.
  new({
    required this._local,
    required this._sync,
    required this._notes,
    required this._folders,
    required this._logger,
  });

  final NotesLocalService _local;
  final NotesSyncService _sync;
  final NotesManager _notes;
  final FoldersManager _folders;
  final MenoLogger _logger;

  /// Deletes a note.
  ///
  /// The row is marked deleted, which takes it out of every list at once, and
  /// the deletion is sent afterwards. The API keeps the row and flips its
  /// deleted flag, so there is nothing to re-read to make the list agree.
  late final Command<Note, void> deleteNote =
      Command.createAsyncNoResult<Note>((note) async {
    await _local.markNoteDeleted(note);
    await _push(note.copyWith(deleted: true), 'the deleted note');
  }, errorFilterFn: menoExceptionFilter, debugName: 'notes.delete');

  /// Pins or unpins a note.
  late final Command<Note, void> togglePin =
      Command.createAsyncNoResult<Note>((note) async {
    final pinned = note.copyWith(pinned: !note.pinned);
    await _notes.remember(pinned);
    await _push(pinned, 'the pin');
  }, errorFilterFn: menoExceptionFilter, debugName: 'notes.pin');

  /// Files a batch of notes into a folder, moving them out of whatever folder
  /// they were in.
  late final Command<({List<Id> noteIds, Id targetFolderId}), void>
  assignNotesToFolder = Command.createAsyncNoResult(
    (({List<Id> noteIds, Id targetFolderId}) args) async {
      if (args.noteIds.isEmpty) return;
      await _move(args.noteIds, folderId: args.targetFolderId);
    },
    errorFilterFn: menoExceptionFilter,
    debugName: 'notes.assignToFolder',
  );

  /// Unfiles a batch of notes from the folder the caller names.
  ///
  /// The folder is named by the caller because that is what it was looking at;
  /// the API unfiles whatever folder the notes are in.
  late final Command<({Id folderId, List<Id> noteIds}), void>
  removeNotesFromFolder = Command.createAsyncNoResult(
    (({List<Id> noteIds, Id folderId}) args) async {
      if (args.noteIds.isEmpty) return;
      await _move(args.noteIds, folderId: null);
    },
    errorFilterFn: menoExceptionFilter,
    debugName: 'notes.removeFromFolder',
  );

  Future<void> _move(List<Id> noteIds, {required Id? folderId}) async {
    final moved = <Note>[];
    for (final id in noteIds) {
      final held = _notes.byId(id);
      if (held == null) continue;
      moved.add(held.copyWith(folderId: folderId, folder: null));
    }

    // The rows are filed first, so both lists agree with the tap before the
    // request leaves.
    await _local.saveNotes(moved, pending: true);

    try {
      await _sync.moveNotes(noteIds, folderId: folderId);

      // One batch, confirmed: the rows are no longer changes waiting to be
      // sent.
      // The versions the API bumped arrive with the read below, which is also
      // what tells the folder cards their new counts.
      await _local.saveNotes(moved, pending: false);
      await _notes.reload.runAsync();
      _folders.reload.run();
    } on Object catch (error, stackTrace) {
      // Still pending, and still filed here: the next push sends each note on
      // its own with the version this device holds.
      _logger.w(
        'The notes could not be moved on the server yet',
        tag: MenoLogTag.network.label,
        error: error,
        stackTrace: stackTrace,
      );
    }
  }

  /// Sends [note]'s change, keeping it pending when it could not be sent.
  Future<void> _push(Note note, String what) async {
    try {
      await _sync.pushNote(note);
    } on Object catch (error, stackTrace) {
      _logger.w(
        'The change could not be sent yet: $what',
        tag: MenoLogTag.network.label,
        error: error,
        stackTrace: stackTrace,
      );
    }
  }

  /// Logs a failure the screen has already handled, without rethrowing.
  ///
  /// Kept for the callers that fire an action and pop in the same frame: the
  /// command reports the error to the global handler, and the screen is gone by
  /// the time a local listener could say anything.
  void logHandled(Object error, StackTrace stackTrace, String what) {
    _logger.w(
      'The notes action "$what" failed',
      tag: MenoLogTag.lifecycle.label,
      error: error,
      stackTrace: stackTrace,
    );
  }

  @override
  FutureOr<dynamic> onDispose() {
    deleteNote.dispose();
    togglePin.dispose();
    assignNotesToFolder.dispose();
    removeNotesFromFolder.dispose();
  }
}
