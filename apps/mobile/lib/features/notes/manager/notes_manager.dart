import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/notes/manager/folders_manager.dart';
import 'package:meno_mobile/features/notes/model/_model.dart';
import 'package:meno_mobile/features/notes/services/_services.dart';

/// The account's notes: the list the notes screen draws, and the cache every
/// other notes manager reads a note from before it edits it.
///
/// # Offline first
///
/// The list is the device's, not the last response's: [notes] is fed by the
/// local store, so it holds what this account has on the phone and survives
/// being closed with no network. [initialize] is the one place that reaches the
/// API — it drains what the last session left unsent and then adopts the
/// server's answer into the store, which flows back through the same stream.
/// Nothing on the screen waits for a request, and a failed request is a pending
/// row rather than an empty screen.
///
/// # Search and the pinned filter
///
/// Both narrow the local query, so a search answers instantly and works with no
/// connection. The rows are the same ones the unfiltered list drew, so a result
/// cannot disagree with the note the user then opens.
///
/// # Why the whole list, held here
///
/// Notes are read a page at a time, but three things need all of them at once:
/// the folder counts on the folders tab, the "pick some notes" modal, and the
/// editor, which has to find the note the user tapped. The store already holds
/// every page, so [NotesManager] reads it once and every write publishes
/// through it — a feed per screen would have meant the same note existing in
/// three lists that could disagree.
final class NotesManager implements Disposable {
  /// Creates the manager over its store, the queue that drains it, and the
  /// folders it resolves against.
  new({
    required this._http,
    required this._local,
    required this._sync,
    required this._folders,
    required this._logger,
  }) {
    _debouncedSearch = searchQuery
        .where((term) => term != _activeKeywords)
        .debounce(const Duration(milliseconds: 400))
        .listen((term, _) {
          _activeKeywords = term;
          _subscribe();
        });

    // A folder renamed, added or deleted changes the tag on a note, so the held
    // notes are re-published with the folders they currently resolve to.
    _folderChanges = _folders.folders.listen((_, _) => _republishFolders());

    _subscribe();
  }

  final NotesHttpService _http;
  final NotesLocalService _local;
  final NotesSyncService _sync;
  final FoldersManager _folders;
  final MenoLogger _logger;

  /// Every note the account owns, newest change first.
  final notes = ListNotifier<Note>(data: []);

  /// How many notes there are, whatever the search and pinned filters narrow
  /// to.
  final totalNotesCount = ValueNotifier<int>(0);

  /// The term the search bar holds.
  final searchQuery = ValueNotifier<String>('');

  /// When non-null, only notes matching this pinned state are shown.
  final pinnedFilter = ValueNotifier<bool?>(null);

  /// Drains what was written offline and adopts the account's notes.
  ///
  /// The push comes first: a row that is still pending is skipped when the
  /// answer is adopted, so sending it before the pull is what lets one round
  /// trip leave the device and the server agreeing.
  late final Command<void, void> initialize =
      Command.createAsyncNoParamNoResult(
    _refreshQuietly,
    errorFilterFn: menoExceptionFilter,
    debugName: 'notes.notes',
  );

  /// The same read, for a caller that wants its failure.
  late final Command<void, void> reload = Command.createAsyncNoParamNoResult(
    _refresh,
    errorFilterFn: menoExceptionFilter,
    debugName: 'notes.notes.reload',
  );

  /// Sets the search term; the debounced listener above narrows the list.
  late final Command<String, void> performSearch =
      Command.createSyncNoResult<String>((term) {
    searchQuery.value = term;
  }, errorFilterFn: menoExceptionFilter);

  ListenableSubscription? _debouncedSearch;
  ListenableSubscription? _folderChanges;
  StreamSubscription<List<Note>>? _watch;

  String _activeKeywords = '';

  /// The note with [id], or null when this account does not hold it.
  ///
  /// Read from the store rather than from [notes], which may be narrowed by a
  /// search the editor was not opened under.
  Note? byId(Id id) {
    final held = _local.note(id);
    if (held == null || held.deleted) return null;
    return _resolve(held);
  }

  /// Stores [note] as a change the server has not confirmed.
  ///
  /// What every action in this feature does first: the list and the editor see
  /// the new value at once, and [NotesSyncService] sends it when it can.
  Future<void> remember(Note note) => _local.saveNote(note, pending: true);

  /// Stores [note] as the server's answer.
  Future<void> adopt(Note note) => _local.saveNote(note, pending: false);

  /// Sets the pinned filter; null shows both pinned and unpinned notes.
  void setPinnedFilter(bool? value) {
    if (pinnedFilter.value == value) return;
    pinnedFilter.value = value;
    _subscribe();
  }

  /// Clears the term and the pinned filter, and shows the full list again.
  void clearSearch() {
    final hadTerm = searchQuery.value.isNotEmpty || _activeKeywords.isNotEmpty;
    final hadPinned = pinnedFilter.value != null;
    if (!hadTerm && !hadPinned) return;

    searchQuery.value = '';
    _activeKeywords = '';
    pinnedFilter.value = null;
    _subscribe();
  }

  /// Follows the rows the current filters select.
  void _subscribe() {
    unawaited(_watch?.cancel());
    _watch = _local
        .watchNotes(
          pinned: pinnedFilter.value,
          keywords: _activeKeywords.isEmpty ? null : _activeKeywords,
        )
        .listen(
          _publish,
          onError: (Object error, StackTrace stackTrace) {
            _logger.w(
              'The notes could not be read from the device',
              tag: MenoLogTag.lifecycle.label,
              error: error,
              stackTrace: stackTrace,
            );
          },
        );
  }

  void _publish(List<Note> rows) {
    final resolved = [for (final row in rows) _resolve(row)];

    notes.startTransAction();
    notes
      ..clear()
      ..addAll(resolved);
    notes.endTransAction();

    _publishCounts();
  }

  /// The note with its folder attached, as far as this client knows it.
  Note _resolve(Note note) {
    final folderId = note.folderId;
    if (folderId == null) return note.copyWith(folder: null);

    final folder = _folders.byId(folderId);
    return note.copyWith(folder: folder);
  }

  /// Re-attaches folders after the folders changed.
  void _republishFolders() {
    if (notes.value.isEmpty) return;

    notes.startTransAction();
    for (var i = 0; i < notes.value.length; i++) {
      notes[i] = _resolve(notes.value[i]);
    }
    notes.endTransAction();
  }

  /// Publishes the account's totals and tells the folders manager how many
  /// notes each one holds.
  ///
  /// Read from the store, not from [notes]: a search narrows what is drawn, it
  /// does not change the account.
  void _publishCounts() {
    final all = _local.allNotes();
    totalNotesCount.value = all.length;

    final counts = <String, int>{};
    for (final note in all) {
      final folderId = note.folderId?.getOrNull;
      if (folderId == null) continue;
      counts[folderId] = (counts[folderId] ?? 0) + 1;
    }
    _folders.applyNoteCounts(counts);
  }

  Future<void> _refreshQuietly() async {
    try {
      await _refresh();
    } on Object catch (error, stackTrace) {
      // The screen draws what the device holds and its retry button; a failed
      // read is not a reason to keep the tab from opening.
      _logger.w(
        'The notes could not be refreshed',
        tag: MenoLogTag.lifecycle.label,
        error: error,
        stackTrace: stackTrace,
      );
    }
  }

  Future<void> _refresh() async {
    await _sync.pushPending();

    final collected = <Note>[];
    String? cursor;

    do {
      final page = await _http.notes(
        cursor: cursor,
        limit: NotesHttpService.maxLimit,
      );
      collected.addAll(page.items);
      cursor = page.hasNextPage ? page.nextCursor : null;
    } while (cursor != null);

    // Every page was read, so the rows it did not name are gone on the server
    // and are dropped here, unless this device holds an unsent change.
    await _local.reconcileNotes(collected, dropStale: true);
    _publishCounts();
  }

  @override
  FutureOr<dynamic> onDispose() {
    _debouncedSearch?.cancel();
    _debouncedSearch = null;
    _folderChanges?.cancel();
    _folderChanges = null;
    unawaited(_watch?.cancel());
    _watch = null;

    notes.dispose();
    totalNotesCount.dispose();
    searchQuery.dispose();
    pinnedFilter.dispose();

    initialize.dispose();
    reload.dispose();
    performSearch.dispose();
  }
}
