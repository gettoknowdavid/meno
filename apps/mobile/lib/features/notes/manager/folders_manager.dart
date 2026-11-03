import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/notes/model/_model.dart';
import 'package:meno_mobile/features/notes/services/_services.dart';

/// The account's folders: the list the notes screen and every folder picker
/// read from.
///
/// # Offline first
///
/// Like the notes, the list is the device's: it is fed by the local store, so a
/// folder created with no network is in the picker at once and is sent later.
/// [initialize] drains the queue and adopts the server's answer; everything the
/// screens read comes back through the same stream.
///
/// # Why the whole list
///
/// Folders are a handful per account and the store keeps every page of them, so
/// a picker can file a note wherever the user meant rather than into the first
/// fifty folders that happened to be read first.
///
/// # Counts
///
/// A folder's `numberOfNotes` is not on the wire. [applyNoteCounts] is where it
/// comes from: the notes manager already holds every note, so it counts them
/// and the folders are re-published with the numbers. No request per folder,
/// and a card's count always matches the list the app is drawing.
final class FoldersManager implements Disposable {
  /// Creates the manager over its store, the queue that drains it, and the API
  /// it pulls from.
  new({
    required this._http,
    required this._local,
    required this._sync,
    required this._logger,
  }) {
    // A search narrows the local query, so it answers with no network and with
    // the rows the unfiltered list would have drawn.
    _debouncedSearch = searchQuery
        .where((term) => term != _activeKeywords)
        .debounce(const Duration(milliseconds: 400))
        .listen((term, _) {
          _activeKeywords = term;
          _subscribe();
        });

    _subscribe();
  }

  final NotesHttpService _http;
  final NotesLocalService _local;
  final NotesSyncService _sync;
  final MenoLogger _logger;

  /// Every folder the account owns, newest first.
  final folders = ListNotifier<NoteFolder>(data: []);

  /// How many folders there are, whatever the current search narrows to.
  final totalFoldersCount = ValueNotifier<int>(0);

  /// The term the search bar holds.
  final searchQuery = ValueNotifier<String>('');

  /// Drains what was written offline and adopts the account's folders.
  late final Command<void, void> initialize =
      Command.createAsyncNoParamNoResult(
    _refreshQuietly,
    errorFilterFn: menoExceptionFilter,
    debugName: 'notes.folders',
  );

  /// The same read, for a caller that wants its failure.
  late final Command<void, void> reload = Command.createAsyncNoParamNoResult(
    _refresh,
    errorFilterFn: menoExceptionFilter,
    debugName: 'notes.folders.reload',
  );

  /// Sets the search term; the debounced listener above narrows the list.
  late final Command<String, void> performSearch =
      Command.createSyncNoResult<String>((term) {
    searchQuery.value = term;
  }, errorFilterFn: menoExceptionFilter);

  /// Deletes a folder, unfiling its notes.
  ///
  /// The folder leaves the list at once and its notes are unfiled locally; the
  /// delete is then sent. A refused or failed delete leaves the folder pending
  /// rather than putting it back: what the user saw happen stays happened until
  /// the server disagrees.
  late final Command<Id, void> deleteFolder =
      Command.createAsyncNoResult<Id>((folderId) async {
    final folder = byId(folderId);
    if (folder == null) {
      throw const NotFoundException(message: 'That folder is not loaded.');
    }

    await _local.markFolderDeleted(folder);
    await _pushDeletion(folder);
  }, errorFilterFn: menoExceptionFilter, debugName: 'notes.deleteFolder');

  ListenableSubscription? _debouncedSearch;
  StreamSubscription<List<NoteFolder>>? _watch;

  String _activeKeywords = '';

  Map<String, int> _counts = const {};

  /// The folder with [id], or null when this account does not hold it.
  NoteFolder? byId(Id id) {
    final held = _local.folder(id);
    if (held == null || held.deleted) return null;
    return held.copyWith(numberOfNotes: _counts[id.getOrNull] ?? 0);
  }

  /// Clears the term and shows the unfiltered list again.
  void clearSearch() {
    if (searchQuery.value.isEmpty && _activeKeywords.isEmpty) return;
    searchQuery.value = '';
    _activeKeywords = '';
    _subscribe();
  }

  /// Replaces the counts the folder cards draw.
  ///
  /// Called by the notes manager after it reads the notes: the same folders,
  /// with the number of notes filed in each.
  void applyNoteCounts(Map<String, int> counts) {
    _counts = counts;
    _republishCounts();
  }

  void _subscribe() {
    unawaited(_watch?.cancel());
    _watch = _local
        .watchFolders(
          keywords: _activeKeywords.isEmpty ? null : _activeKeywords,
        )
        .listen(
          _publish,
          onError: (Object error, StackTrace stackTrace) {
            _logger.w(
              'The folders could not be read from the device',
              tag: MenoLogTag.lifecycle.label,
              error: error,
              stackTrace: stackTrace,
            );
          },
        );
  }

  void _publish(List<NoteFolder> rows) {
    folders.startTransAction();
    folders
      ..clear()
      ..addAll(rows.map(_withCount));
    folders.endTransAction();

    totalFoldersCount.value = _local.countFolders();
  }

  /// Re-applies the counts after they changed, without re-reading the store.
  void _republishCounts() {
    final current = folders.value;
    if (current.isEmpty) return;

    folders.startTransAction();
    for (var i = 0; i < current.length; i++) {
      folders[i] = _withCount(current[i]);
    }
    folders.endTransAction();
  }

  NoteFolder _withCount(NoteFolder folder) =>
      folder.copyWith(numberOfNotes: _counts[folder.id.getOrNull] ?? 0);

  Future<void> _refreshQuietly() async {
    try {
      await _refresh();
    } on Object catch (error, stackTrace) {
      // The screen draws what the device holds and its retry button; a failed
      // read is not a reason to keep the whole notes tab from opening.
      _logger.w(
        'The folders could not be refreshed',
        tag: MenoLogTag.lifecycle.label,
        error: error,
        stackTrace: stackTrace,
      );
    }
  }

  Future<void> _refresh() async {
    final collected = <NoteFolder>[];
    String? cursor;

    do {
      final page = await _http.folders(
        cursor: cursor,
        limit: NotesHttpService.maxLimit,
      );

      collected.addAll(page.items);
      cursor = page.hasNextPage ? page.nextCursor : null;
    } while (cursor != null);

    await _local.reconcileFolders(collected, dropStale: true);
    totalFoldersCount.value = _local.countFolders();
  }

  Future<void> _pushDeletion(NoteFolder folder) async {
    try {
      await _sync.pushFolder(folder.copyWith(deleted: true));
    } on Object catch (error, stackTrace) {
      // The folder is already gone from the device and queued; the next push
      // sends the deletion again.
      _logger.w(
        'The deleted folder could not be sent yet',
        tag: MenoLogTag.network.label,
        error: error,
        stackTrace: stackTrace,
      );
    }
  }

  @override
  FutureOr<dynamic> onDispose() {
    _debouncedSearch?.cancel();
    _debouncedSearch = null;
    unawaited(_watch?.cancel());
    _watch = null;

    folders.dispose();
    totalFoldersCount.dispose();
    searchQuery.dispose();

    initialize.dispose();
    reload.dispose();
    performSearch.dispose();
    deleteFolder.dispose();
  }
}
