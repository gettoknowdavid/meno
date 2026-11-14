import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/notes/manager/notes_manager.dart';
import 'package:meno_mobile/features/notes/model/_model.dart';
import 'package:meno_mobile/features/notes/services/_services.dart';

/// Autosave fires this long after the last keystroke.
const _kAutoSaveDebounce = Duration(milliseconds: 1500);

/// The note on screen, and the writes that keep it.
///
/// # Where a note comes from
///
/// Nothing in the API answers "one note by id": a note is read as part of the
/// account's list, and the device already holds every one of them. So the
/// editor reads [NotesManager]'s copy — the row the list drew, with the
/// `version` the next write is guarded on. A note that is not held is a note
/// this account does not have, which is the error the screen shows.
///
/// # How it saves
///
/// A save lands in the local store first, so autosave works with no network and
/// a note typed and closed is never lost. The row is then queued, and
/// [NotesSyncService] decides whether it is a create — the first time the
/// note reaches the server — or a patch quoting the version the last answer
/// carried.
/// The queue survives a restart, which a timer in memory could not.
final class NoteEditorManager implements Disposable {
  /// Creates the editor over [_noteId], or a new note when it is null.
  new({
    required this._sync,
    required this._notes,
    required this._logger,
    required this._noteId,
  });

  final NotesSyncService _sync;
  final NotesManager _notes;
  final MenoLogger _logger;
  final Id? _noteId;

  Timer? _debounce;

  /// The note being edited.
  final note = ValueNotifier<Note>(.empty);

  /// How far the last save got.
  final status = ValueNotifier<NoteEditorStatus>(.idle);

  /// Whether this editor was opened on a note that already exists.
  late final isEdit = ValueNotifier<bool>(_noteId != null);

  /// Loads the note, or primes a blank one with a fresh id.
  late final Command<void, void> initialize =
      Command.createSyncNoParamNoResult(() {
    final noteId = _noteId;

    if (noteId == null) {
      // The id is chosen here rather than by the server: a note written offline
      // keeps the id it was created with, and the API takes the client's.
      note.value = Note.fromNewId(Id.unique());
      return;
    }

    final held = _notes.byId(noteId);
    if (held == null) {
      throw const NotFoundException(message: 'That note is not loaded.');
    }

    note.value = held;
  }, errorFilterFn: menoExceptionFilter, debugName: 'notes.editor');

  /// Records a new title and schedules the save.
  void onTitleChanged(String value) {
    note.value = note.value.copyWith(title: SingleLineString(value));
    _markDirtyAndScheduleAutoSave();
  }

  /// Records new body content (a Quill delta, as JSON) and schedules the save.
  void onContentChanged(String deltaJson) {
    note.value = note.value.copyWith(content: MultiLineString(deltaJson));
    _markDirtyAndScheduleAutoSave();
  }

  /// Saves now, cancelling the pending autosave.
  ///
  /// What the page calls when it is popped and when the app is backgrounded:
  /// the debounce is a delay, not a promise that the timer will run.
  Future<void> saveNow() async {
    _debounce?.cancel();
    await _persist();
  }

  void _markDirtyAndScheduleAutoSave() {
    status.value = .dirty;
    _debounce?.cancel();
    _debounce = Timer(_kAutoSaveDebounce, () => unawaited(_persist()));
  }

  Future<void> _persist() async {
    // A save already in flight owns the next `version`; a second one started
    // from the same base would be refused as stale.
    if (status.value.isSaving) return;

    final current = note.value;
    if (!current.isValid) return;

    status.value = .saving;

    // Stored first: this is the save that makes the text durable. What follows
    // is delivery, and delivery can wait for a network.
    await _notes.remember(current);
    status.value = .saved;

    unawaited(_deliver(current));
  }

  /// Sends what was just stored, and takes back the concurrency fields.
  ///
  /// A refused or failed send is logged and left in the queue: the text is on
  /// the device, the editor must not claim it lost it, and the next push tries
  /// again.
  Future<void> _deliver(Note sent) async {
    try {
      final saved = await _sync.pushNote(sent);

      // Whatever arrived is the server's answer about this note, but the
      // editor may have moved on: only the fields that decide the next write
      // are taken from it, never the text.
      final draft = note.value;
      if (draft.id != saved.id) return;
      note.value = draft.copyWith(
        version: saved.version,
        createdAt: saved.createdAt,
        updatedAt: saved.updatedAt,
      );
    } on Object catch (error, stackTrace) {
      _logger.w(
        'The note is saved on the device and could not be sent yet',
        tag: MenoLogTag.network.label,
        error: error,
        stackTrace: stackTrace,
      );
    }
  }

  @override
  FutureOr<dynamic> onDispose() {
    _debounce?.cancel();
    _debounce = null;

    note.dispose();
    status.dispose();
    isEdit.dispose();
    initialize.dispose();
  }
}

/// The save state of the note editor.
///
/// [idle] is the initial state, [dirty] is when the note is modified, [saving]
/// while a write is in flight, [saved] once the device holds it, and [failure]
/// when the write could not be stored at all.
enum NoteEditorStatus { idle, dirty, saving, saved, failure }

/// Whether a status means what it says.
extension NoteEditorStatusX on NoteEditorStatus {
  /// Nothing has been typed yet.
  bool get isIdle => this == NoteEditorStatus.idle;

  /// There is text the editor has not stored yet.
  bool get isDirty => this == NoteEditorStatus.dirty;

  /// A write is in flight.
  bool get isSaving => this == NoteEditorStatus.saving;

  /// The device holds the latest text.
  bool get isSaved => this == NoteEditorStatus.saved;

  /// The last write did not land.
  bool get isFailure => this == NoteEditorStatus.failure;
}
