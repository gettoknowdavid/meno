import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/notes/manager/folders_manager.dart';
import 'package:meno_mobile/features/notes/model/_model.dart';
import 'package:meno_mobile/features/notes/services/_services.dart';

/// One folder, with the notes filed in it.
///
/// # Why this is its own manager
///
/// The folder page is a screen with its own search and its own selection of
/// notes to add or remove. Sharing the notes manager's filters would mean
/// sharing them with the tab behind it: typing in one would narrow the other.
/// This holds its own filters over the same local store, and is registered in a
/// scope that lives as long as the page.
///
/// # Offline first
///
/// The notes arrive from the store, filtered by this folder, so the page draws
/// what the device holds — including a note filed here a minute ago with no
/// network. [initialize] pulls this folder's page and adopts it; nothing the
/// page draws waits for that request.
///
/// # Where the folder comes from
///
/// There is no "read one folder" endpoint — a folder is read as part of the
/// account's list. So the folder is [FoldersManager]'s copy, re-read when the
/// list changes: a rename made from the editor shows here without the page
/// asking for anything.
final class FolderManager implements Disposable {
  /// Creates the manager for [_folderId].
  new({
    required this._http,
    required this._local,
    required this._folders,
    required this._logger,
    required this._folderId,
  }) {
    _debouncedSearch = searchQuery
        .where((term) => term != _activeKeywords)
        .debounce(const Duration(milliseconds: 400))
        .listen((term, _) {
          _activeKeywords = term;
          _subscribe();
        });

    _folderChanges = _folders.folders.listen((_, _) => _adoptFolder());
  }

  final NotesHttpService _http;
  final NotesLocalService _local;
  final FoldersManager _folders;
  final MenoLogger _logger;
  final Id _folderId;

  /// The folder this page is about.
  final folder = ValueNotifier<NoteFolder>(.empty);

  /// The notes filed in it, newest change first.
  final notes = ListNotifier<Note>(data: []);

  /// The term the search bar holds.
  final searchQuery = ValueNotifier<String>('');

  /// How many notes are filed in it, whatever the search narrows to.
  final totalNotesCount = ValueNotifier<int>(0);

  /// Reads the folder and its notes.
  late final Command<void, void> initialize =
      Command.createAsyncNoParamNoResult(
    _initialize,
    errorFilterFn: menoExceptionFilter,
    debugName: 'notes.folder',
  );

  /// Re-reads the notes, for a caller that wants the failure.
  late final Command<void, void> reload = Command.createAsyncNoParamNoResult(
    _load,
    errorFilterFn: menoExceptionFilter,
    debugName: 'notes.folder.reload',
  );

  /// Sets the search term; the debounced listener above narrows the list.
  late final Command<String, void> performSearch =
      Command.createSync<String, void>(
    _onSearchChanged,
    initialValue: null,
    errorFilterFn: menoExceptionFilter,
  );

  ListenableSubscription? _debouncedSearch;
  ListenableSubscription? _folderChanges;
  StreamSubscription<List<Note>>? _watch;

  String _activeKeywords = '';

  Future<void> _initialize() async {
    // A deep link can open a folder this client has not listed yet.
    final held = _folders.byId(_folderId);
    if (held == null) await _folders.initialize.runAsync();

    _adoptFolder();
    _subscribe();
    await _load();
  }

  /// Takes the folder from the list, or says so when it is gone.
  void _adoptFolder() {
    final held = _folders.byId(_folderId);
    if (held == null) {
      if (folder.value.id != _folderId) {
        // Not an error yet: the list may still be loading. The page draws its
        // empty state until a folder with this id arrives.
        folder.value = NoteFolder.empty;
      }
      return;
    }
    folder.value = held;
    _republish();
  }

  void _onSearchChanged(String query) {
    searchQuery.value = query;
  }

  void _subscribe() {
    unawaited(_watch?.cancel());
    _watch = _local
        .watchNotesInFolder(
          _folderId,
          keywords: _activeKeywords.isEmpty ? null : _activeKeywords,
        )
        .listen(
          _publish,
          onError: (Object error, StackTrace stackTrace) {
            _logger.w(
              'The notes in a folder could not be read from the device',
              tag: MenoLogTag.lifecycle.label,
              error: error,
              stackTrace: stackTrace,
            );
          },
        );
  }

  void _publish(List<Note> rows) {
    notes.startTransAction();
    notes
      ..clear()
      ..addAll(rows.map(_withFolder));
    notes.endTransAction();

    totalNotesCount.value = _local.countNotesInFolder(_folderId);
  }

  /// Re-attaches the folder after the folder list changed.
  void _republish() {
    for (var i = 0; i < notes.value.length; i++) {
      notes[i] = _withFolder(notes.value[i]);
    }
  }

  Note _withFolder(Note note) => note.copyWith(
    folder: folder.value.id == _folderId ? folder.value : null,
  );

  /// Adopts this folder's page from the API into the store.
  ///
  /// Only this page is read, so stale rows are kept: a row the page does not
  /// name may be a row another page holds.
  Future<void> _load() async {
    String? cursor;

    try {
      do {
        final page = await _http.notes(
          folderId: _folderId,
          cursor: cursor,
          limit: NotesHttpService.maxLimit,
        );

        await _local.reconcileNotes(page.items);
        cursor = page.hasNextPage ? page.nextCursor : null;
      } while (cursor != null);
    } on Object catch (error, stackTrace) {
      _logger.w(
        'The notes in a folder could not be refreshed',
        tag: MenoLogTag.network.label,
        error: error,
        stackTrace: stackTrace,
      );
      rethrow;
    }
  }

  @override
  FutureOr<dynamic> onDispose() {
    _debouncedSearch?.cancel();
    _debouncedSearch = null;
    _folderChanges?.cancel();
    _folderChanges = null;
    unawaited(_watch?.cancel());
    _watch = null;

    folder.dispose();
    notes.dispose();
    searchQuery.dispose();
    totalNotesCount.dispose();

    initialize.dispose();
    reload.dispose();
    performSearch.dispose();
  }
}
