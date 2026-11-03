import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/features/broadcast/services/broadcast_rows.dart';
import 'package:meno_mobile/features/notes/model/note_rows.dart';
import 'package:meno_mobile/objectbox.g.dart';
import 'package:path/path.dart' as p;
import 'package:path_provider/path_provider.dart';

/// The device's ObjectBox store, opened once for the app's lifetime.
///
/// One store holds every feature's local tables, so they share a transaction
/// log and there is exactly one place that opens and closes the database. Each
/// feature reaches its own boxes through the accessors here rather than opening
/// a store of its own.
///
/// Opening is also re-entrant: `Store.isOpen`/`Store.attach` lets a second call
/// attach to the store a first one already opened, which is what keeps a hot
/// restart from failing on a locked directory.
final class Database implements Disposable {
  new _(this._store) {
    _draftBox = _store.box<BroadcastDraftRow>();
    _favouriteBroadcastBox = _store.box<FavouriteBroadcastRow>();
    _noteBox = _store.box<NoteRow>();
    _noteFolderBox = _store.box<NoteFolderRow>();
  }

  /// Opens a database over an already-open [store].
  ///
  /// The app opens its store in the documents directory; a test opens one in
  /// memory and hands it here, so the services under test use the same
  /// [Database] type instead of a second, test-only one.
  @visibleForTesting
  factory fromStore(Store store) => ._(store);

  final Store _store;

  late final Box<BroadcastDraftRow> _draftBox;
  late final Box<FavouriteBroadcastRow> _favouriteBroadcastBox;
  late final Box<NoteRow> _noteBox;
  late final Box<NoteFolderRow> _noteFolderBox;

  /// Opens the store in the app's documents directory, under `meno`.
  static Future<Database> create() async {
    final docsDir = await getApplicationDocumentsDirectory();
    final directory = p.join(docsDir.path, 'meno');

    final store = Store.isOpen(directory)
        ? Store.attach(getObjectBoxModel(), directory)
        : await openStore(directory: directory);

    return Database._(store);
  }

  /// The drafts one account is writing.
  Box<BroadcastDraftRow> get draftBox => _draftBox;

  /// The broadcasts accounts keep for later.
  Box<FavouriteBroadcastRow> get favouriteBroadcastBox =>
      _favouriteBroadcastBox;

  /// The account's notes, as the device holds them.
  Box<NoteRow> get noteBox => _noteBox;

  /// The account's folders.
  Box<NoteFolderRow> get noteFolderBox => _noteFolderBox;

  /// Runs a synchronous write transaction.
  T runWriteTx<T>(T Function() fn) => _store.runInTransaction(.write, fn);

  /// Runs a write transaction on ObjectBox's background thread pool.
  ///
  /// The one to prefer for bulk writes, such as seeding the verses of a
  /// translation, because it does not block the isolate the UI runs on.
  Future<R> runWriteTxAsync<R, P>(
    R Function(Store store, P parameter) callback,
    P param,
  ) => _store.runInTransactionAsync(.write, callback, param);

  @override
  FutureOr<dynamic> onDispose() {
    _store.close();
  }
}
