// 

import 'package:meno_mobile/_core/_core.dart' as core;
import 'package:meno_mobile/_shared/services/database.dart';
import 'package:meno_mobile/features/notes/model/_model.dart';
import 'package:meno_mobile/objectbox.g.dart';

/// What the notes screens keep on the device.
///
/// # Why a local store
///
/// The feature is offline-first: a note is written here first — the list is
/// drawn from this store, not from the last response — and the API is told
/// afterwards. A note typed on a train therefore appears immediately, survives
/// being closed, and is sent when there is a network again. Nothing on the
/// screens waits for a request, and a failed request is a pending row rather
/// than lost text.
///
/// # The owner is part of the service, not an argument
///
/// The owner is fixed when the service is created — it lives in the account
/// scope — so a caller cannot name one, and a row belonging to somebody else is
/// refused with [core.OwnerViolationException] rather than returned or deleted.
/// No `Box` is handed out either: the store is the only thing that reads or
/// writes these rows.
///
/// # Pending rows
///
/// Every write takes a `pending` flag. A change this device made is pending
/// until the server confirms it; the sync service reads the pending rows back
/// and pushes them. The flag is stored, so an interrupted session resumes with
/// the same queue.
class NotesLocalService {
  /// Creates the store for one account.
  ///
  /// An invalid owner is refused here instead of being flattened into rows that
  /// belong to nobody.
  new(this._ownerId, this._database, {required this._logger}) {
    if (!_ownerId.isValid) {
      throw const core.OwnerViolationException(
        message: 'A notes store needs the account it belongs to.',
      );
    }
  }

  final core.Id _ownerId;
  final Database _database;
  final core.MenoLogger _logger;

  /// The owner every row of this service belongs to.
  core.Id get ownerId => _ownerId;

  /// Notes that are not deleted, newest change first, rebuilt whenever a row
  /// changes — including the ones the sync service confirms in the background.
  ///
  /// [pinned] and [keywords] narrow the query itself, so a search is a local
  /// read that answers with no network, and the list the screen draws is always
  /// what the device holds.
  Stream<List<Note>> watchNotes({bool? pinned, String? keywords}) {
    return _noteQuery(pinned: pinned, keywords: keywords)
        .watch(triggerImmediately: true)
        .map((query) => query.find().map((row) => row.toDomain).toList());
  }

  /// Notes filed in one folder, newest change first.
  Stream<List<Note>> watchNotesInFolder(core.Id folderId, {String? keywords}) {
    var condition = _ownedNotes & NoteRow_.folderId.equals(folderId.getOrCrash);
    final term = _term(keywords);
    if (term != null) condition = condition & _keywordCondition(term);

    return _database.noteBox
        .query(_visibleNotes(condition))
        .order(NoteRow_.lastModifiedMillis, flags: Order.descending)
        .watch(triggerImmediately: true)
        .map((query) => query.find().map((row) => row.toDomain).toList());
  }

  /// Folders that are not deleted, newest change first.
  Stream<List<NoteFolder>> watchFolders({String? keywords}) {
    var condition = _ownedFolders;
    final term = _term(keywords);
    if (term != null) {
      condition =
          condition & NoteFolderRow_.title.contains(term, caseSensitive: false);
    }

    return _database.noteFolderBox
        .query(_visibleFolders(condition))
        .order(NoteFolderRow_.lastModifiedMillis, flags: Order.descending)
        .watch(triggerImmediately: true)
        .map((query) => query.find().map((row) => row.toDomain).toList());
  }

  /// Every note the account holds, deleted ones included.
  ///
  /// The counts and the folder totals are read from here rather than from a
  /// filtered stream: a search narrows the list, it does not change the
  /// account.
  List<Note> allNotes() {
    final query = _ownedNotesNotDeleted.build();
    try {
      return query.find().map((row) => row.toDomain).toList();
    } finally {
      query.close();
    }
  }

  /// How many notes the account holds.
  int countNotes() {
    final query = _ownedNotesNotDeleted.build();
    try {
      return query.count();
    } finally {
      query.close();
    }
  }

  /// How many folders the account holds.
  int countFolders() {
    final query = _ownedFoldersNotDeleted.build();
    try {
      return query.count();
    } finally {
      query.close();
    }
  }

  /// The note with [id], or null when this account does not hold it.
  Note? note(core.Id id) {
    final row = _firstNote(id.getOrCrash);
    if (row == null) return null;
    _requireOwner(row.ownerId);
    return row.toDomain;
  }

  /// The folder with [id], or null when this account does not hold it.
  NoteFolder? folder(core.Id id) {
    final row = _firstFolder(id.getOrCrash);
    if (row == null) return null;
    _requireOwner(row.ownerId);
    return row.toDomain;
  }

  /// Notes holding a change the server has not confirmed, deletion included.
  List<Note> pendingNotes() {
    final query = _database.noteBox
        .query(_ownedNotes & NoteRow_.pending.equals(true))
        .build();
    try {
      return query.find().map((row) => row.toDomain).toList();
    } finally {
      query.close();
    }
  }

  /// Folders holding a change the server has not confirmed.
  List<NoteFolder> pendingFolders() {
    final query = _database.noteFolderBox
        .query(_ownedFolders & NoteFolderRow_.pending.equals(true))
        .build();
    try {
      return query.find().map((row) => row.toDomain).toList();
    } finally {
      query.close();
    }
  }

  /// Stores [note] as this account's row.
  Future<void> saveNote(Note note, {required bool pending}) async {
    await _putNote(note, pending: pending);
  }

  /// Stores [notes] as this account's rows, in one transaction.
  Future<void> saveNotes(List<Note> notes, {required bool pending}) async {
    if (notes.isEmpty) return;

    await _database.runWriteTxAsync(
      (store, rows) => store.box<NoteRow>().putMany(rows),
      [for (final note in notes) _row(note, pending: pending)],
    );
  }

  /// Stores [folder] as this account's row.
  Future<void> saveFolder(NoteFolder folder, {required bool pending}) async {
    await _putFolder(folder, pending: pending);
  }

  /// Stores [folders] as this account's rows, in one transaction.
  Future<void> saveFolders(
    List<NoteFolder> folders, {
    required bool pending,
  }) async {
    if (folders.isEmpty) return;

    await _database.runWriteTxAsync(
      (store, rows) => store.box<NoteFolderRow>().putMany(rows),
      [for (final folder in folders) _folderRow(folder, pending: pending)],
    );
  }

  /// Adopts a page of notes the server answered with.
  ///
  /// A row that is still pending is left alone: the unsent text is younger than
  /// the answer, and replacing it is exactly the lost note this store exists to
  /// prevent. When every page has been read, [dropStale] removes the rows the
  /// server no longer lists — a note deleted on another device leaves the list
  /// here too — while a pending row is kept for the next push.
  Future<void> reconcileNotes(
    List<Note> remote, {
    bool dropStale = false,
  }) async {
    final held = _rowsById<NoteRow>(
      _database.noteBox.query(_ownedNotes).build().find(),
      (row) => row.noteId,
    );

    final rows = <NoteRow>[];
    for (final note in remote) {
      final existing = held[note.id.getOrNull];
      if (existing != null && existing.pending) continue;

      rows.add(
        note.toRow(
          ownerId: _ownerKey,
          lastModifiedMillis: _serverStamp(note.updatedAt, note.createdAt),
        ),
      );
    }

    if (rows.isNotEmpty) {
      await _database.runWriteTxAsync(
        (store, rows) => store.box<NoteRow>().putMany(rows),
        rows,
      );
    }

    if (!dropStale) return;

    final remoteIds = {for (final note in remote) note.id.getOrNull};
    final stale = held.values
        .where((row) => !row.pending && !remoteIds.contains(row.noteId))
        .toList();
    if (stale.isEmpty) return;

    await _database.runWriteTxAsync(
      (store, ids) => store.box<NoteRow>().removeMany(ids),
      [for (final row in stale) row.id],
    );
  }

  /// Adopts a page of folders the server answered with, the same way.
  Future<void> reconcileFolders(
    List<NoteFolder> remote, {
    bool dropStale = false,
  }) async {
    final held = _rowsById<NoteFolderRow>(
      _database.noteFolderBox.query(_ownedFolders).build().find(),
      (row) => row.folderId,
    );

    final rows = <NoteFolderRow>[];
    for (final folder in remote) {
      final existing = held[folder.id.getOrNull];
      if (existing != null && existing.pending) continue;

      rows.add(
        folder.toRow(
          ownerId: _ownerKey,
          lastModifiedMillis: _serverStamp(folder.updatedAt, folder.createdAt),
        ),
      );
    }

    if (rows.isNotEmpty) {
      await _database.runWriteTxAsync(
        (store, rows) => store.box<NoteFolderRow>().putMany(rows),
        rows,
      );
    }

    if (!dropStale) return;

    final remoteIds = {for (final folder in remote) folder.id.getOrNull};
    final stale = held.values
        .where((row) => !row.pending && !remoteIds.contains(row.folderId))
        .toList();
    if (stale.isEmpty) return;

    await _database.runWriteTxAsync(
      (store, ids) => store.box<NoteFolderRow>().removeMany(ids),
      [for (final row in stale) row.id],
    );
  }

  /// Marks [note] as deleted, still pending.
  ///
  /// The row is kept rather than removed: the deletion is a change the server
  /// has not seen yet, and a row that was deleted offline has to be sent when
  /// the device is online again. The list stops drawing it at once, because
  /// every watch query excludes deleted rows.
  Future<void> markNoteDeleted(Note note) async {
    await _putNote(note.copyWith(deleted: true), pending: true);
  }

  /// Marks [folder] as deleted and unfiles its notes, both pending.
  ///
  /// The API deletes a folder's notes with it only when asked to; the screens
  /// do not ask, so a folder that goes leaves its notes unfiled — the same
  /// thing the device does to the rows it holds.
  Future<void> markFolderDeleted(NoteFolder folder) async {
    await _putFolder(folder.copyWith(deleted: true), pending: true);

    final key = folder.id.getOrCrash;
    final filed = _database.noteBox
        .query(_ownedNotes & NoteRow_.folderId.equals(key))
        .build();
    try {
      final orphans = filed.find();
      if (orphans.isEmpty) return;

      final now = DateTime.now().millisecondsSinceEpoch;
      for (final note in orphans) {
        note
          ..folderId = null
          ..pending = true
          ..lastModifiedMillis = now;
      }
      await _database.noteBox.putManyAsync(orphans);
    } finally {
      filed.close();
    }
  }

  /// How many notes are filed in [folderId].
  int countNotesInFolder(core.Id folderId) {
    final query = _database.noteBox
        .query(_ownedNotes & NoteRow_.folderId.equals(folderId.getOrCrash))
        .build();
    try {
      return query.count();
    } finally {
      query.close();
    }
  }

  /// Forgets the note with [id] — used once the server confirmed a deletion.
  Future<void> removeNote(core.Id id) async {
    final row = _firstNote(id.getOrCrash);
    if (row == null) return;
    _requireOwner(row.ownerId);
    await _database.noteBox.removeAsync(row.id);
  }

  /// Forgets the folder with [id], detaching the notes filed in it.
  ///
  /// The API unfiles a folder's notes when the folder goes, so the device does
  /// the same: a note must not be filed in a folder that no longer exists.
  Future<void> removeFolder(core.Id id, {bool unfileNotes = true}) async {
    final row = _firstFolder(id.getOrCrash);
    if (row == null) return;
    _requireOwner(row.ownerId);

    if (unfileNotes) {
      final key = id.getOrCrash;
      final filed = _database.noteBox
          .query(_ownedNotes & NoteRow_.folderId.equals(key))
          .build();
      try {
        final orphans = filed.find();
        for (final note in orphans) {
          note
            ..folderId = null
            ..pending = true
            ..lastModifiedMillis = DateTime.now().millisecondsSinceEpoch;
        }
        if (orphans.isNotEmpty) await _database.noteBox.putManyAsync(orphans);
      } finally {
        filed.close();
      }
    }

    await _database.noteFolderBox.removeAsync(row.id);
  }

  /// Forgets every row this account owns.
  ///
  /// Called when the account leaves the device. Rows of other accounts are
  /// untouched: they belong to a store this service cannot see.
  Future<void> clear() async {
    final notes = _database.noteBox
        .query(NoteRow_.ownerId.equals(_ownerKey))
        .build();
    final folders = _database.noteFolderBox
        .query(NoteFolderRow_.ownerId.equals(_ownerKey))
        .build();
    try {
      final noteIds = notes.findIds();
      final folderIds = folders.findIds();

      _database.runWriteTx(() {
        _database.noteBox.removeMany(noteIds);
        _database.noteFolderBox.removeMany(folderIds);
      });
    } finally {
      notes.close();
      folders.close();
    }
  }

  /// The account id as every row stores it.
  String get _ownerKey => _ownerId.getOrCrash;

  Condition<NoteRow> get _ownedNotes => NoteRow_.ownerId.equals(_ownerKey);

  Condition<NoteFolderRow> get _ownedFolders =>
      NoteFolderRow_.ownerId.equals(_ownerKey);

  QueryBuilder<NoteRow> get _ownedNotesNotDeleted =>
      _database.noteBox.query(_visibleNotes(_ownedNotes));

  QueryBuilder<NoteFolderRow> get _ownedFoldersNotDeleted =>
      _database.noteFolderBox.query(_visibleFolders(_ownedFolders));

  Condition<NoteRow> _visibleNotes(Condition<NoteRow> condition) =>
      condition & NoteRow_.deleted.equals(false);

  Condition<NoteFolderRow> _visibleFolders(
    Condition<NoteFolderRow> condition,
  ) => condition & NoteFolderRow_.deleted.equals(false);

  QueryBuilder<NoteRow> _noteQuery({bool? pinned, String? keywords}) {
    var condition = _visibleNotes(_ownedNotes);
    if (pinned != null) {
      condition = condition & NoteRow_.pinned.equals(pinned);
    }

    final term = _term(keywords);
    if (term != null) condition = condition & _keywordCondition(term);

    return _database.noteBox
        .query(condition)
        .order(NoteRow_.lastModifiedMillis, flags: Order.descending);
  }

  Condition<NoteRow> _keywordCondition(String term) => NoteRow_.title
      .contains(term, caseSensitive: false)
      .or(NoteRow_.content.contains(term, caseSensitive: false));

  Future<void> _putNote(Note note, {required bool pending}) async {
    final stored = _firstNote(note.id.getOrCrash);
    if (stored != null) _requireOwner(stored.ownerId);
    await _database.noteBox.putAsync(_row(note, pending: pending));
  }

  Future<void> _putFolder(NoteFolder folder, {required bool pending}) async {
    final stored = _firstFolder(folder.id.getOrCrash);
    if (stored != null) _requireOwner(stored.ownerId);
    await _database.noteFolderBox.putAsync(
      _folderRow(folder, pending: pending),
    );
  }

  NoteRow _row(Note note, {required bool pending}) =>
      note.toRow(ownerId: _ownerKey, pending: pending);

  NoteFolderRow _folderRow(NoteFolder folder, {required bool pending}) =>
      folder.toRow(ownerId: _ownerKey, pending: pending);

  NoteRow? _firstNote(String noteId) {
    final query = _database.noteBox
        .query(NoteRow_.noteId.equals(noteId))
        .build();
    try {
      return query.findFirst();
    } finally {
      query.close();
    }
  }

  NoteFolderRow? _firstFolder(String folderId) {
    final query = _database.noteFolderBox
        .query(NoteFolderRow_.folderId.equals(folderId))
        .build();
    try {
      return query.findFirst();
    } finally {
      query.close();
    }
  }

  static Map<String, T> _rowsById<T>(List<T> rows, String Function(T row) id) {
    final byId = <String, T>{};
    for (final row in rows) {
      byId[id(row)] = row;
    }
    return byId;
  }

  /// Where the server's answer sorts a row: its last write, else its creation,
  /// else now for a row that says neither.
  static int _serverStamp(DateTime? updatedAt, DateTime? createdAt) {
    final stamp = updatedAt ?? createdAt;
    return stamp?.millisecondsSinceEpoch ??
        DateTime.now().millisecondsSinceEpoch;
  }

  static String? _term(String? keywords) {
    final trimmed = keywords?.trim();
    return (trimmed == null || trimmed.isEmpty) ? null : trimmed;
  }

  /// Throws unless [storedOwnerId] is this service's owner.
  void _requireOwner(String storedOwnerId) {
    if (storedOwnerId == _ownerKey) return;
    _logger.w(
      'Refused a row owned by another account',
      tag: core.MenoLogTag.di.label,
    );
    throw const core.OwnerViolationException();
  }
}
