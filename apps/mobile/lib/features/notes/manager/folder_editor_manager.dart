import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/notes/manager/folders_manager.dart';
import 'package:meno_mobile/features/notes/model/_model.dart';
import 'package:meno_mobile/features/notes/services/_services.dart';

/// The folder being created or renamed.
///
/// The same two writes as the note editor, without an autosave: a folder is a
/// title, and it is submitted when the user says so. The scope that holds it
/// lives as long as the modal that opened it.
///
/// # Offline first
///
/// A submit is stored before it is sent, so the folder is in every picker at
/// once and a submit with no network is a pending row rather than a failure.
/// The sync service sends it when it can, and the list follows the store either
/// way.
final class FolderEditorManager implements Disposable {
  /// Creates the editor over [_folderId], or a new folder when it is null.
  new({
    required this._local,
    required this._sync,
    required this._folders,
    required this._logger,
    required this._folderId,
  });

  final NotesLocalService _local;
  final NotesSyncService _sync;
  final FoldersManager _folders;
  final MenoLogger _logger;
  final Id? _folderId;

  /// The folder being edited.
  final folder = ValueNotifier<NoteFolder>(.empty);

  /// Whether this editor was opened on a folder that already exists.
  final isEdit = ValueNotifier<bool>(false);

  /// Loads the folder, or primes a blank one with a fresh id.
  late final Command<void, void> initialize =
      Command.createSyncNoParamNoResult(() {
    final folderId = _folderId;

    if (folderId == null) {
      folder.value = NoteFolder.fromNewId(Id.unique());
      return;
    }

    final held = _folders.byId(folderId);
    if (held == null) {
      throw const NotFoundException(message: 'That folder is not loaded.');
    }

    isEdit.value = true;
    folder.value = held;
  }, errorFilterFn: menoExceptionFilter, debugName: 'notes.folderEditor');

  /// Records a new title.
  void onTitleChanged(String value) {
    folder.value = folder.value.copyWith(title: SingleLineString(value));
  }

  /// Stores the folder, then sends it.
  ///
  /// The submit itself does not wait for the network: the folder exists on the
  /// device the moment the user confirms, which is what lets the modal close
  /// and the picker behind it already offer it.
  late final Command<void, void> submit = Command.createAsyncNoParamNoResult(
    () async {
      final current = folder.value;
      if (!current.isValid) return;

      await _local.saveFolder(current, pending: true);
      folder.value = current;

      await _deliver(current);
    },
    errorFilterFn: menoExceptionFilter,
    debugName: 'notes.folderEditor.submit',
  );

  Future<void> _deliver(NoteFolder sent) async {
    try {
      final saved = await _sync.pushFolder(sent);

      // The version the next write is guarded on comes from the answer; the
      // title stays whatever the editor holds.
      if (folder.value.id != saved.id) return;
      folder.value = folder.value.copyWith(
        version: saved.version,
        createdAt: saved.createdAt,
        updatedAt: saved.updatedAt,
      );
    } on Object catch (error, stackTrace) {
      _logger.w(
        'The folder is saved on the device and could not be sent yet',
        tag: MenoLogTag.network.label,
        error: error,
        stackTrace: stackTrace,
      );
    }
  }

  /// Logs a failed submit that the caller has already reported.
  void logHandled(Object error, StackTrace stackTrace) {
    _logger.w(
      'The folder could not be saved',
      tag: MenoLogTag.lifecycle.label,
      error: error,
      stackTrace: stackTrace,
    );
  }

  @override
  FutureOr<dynamic> onDispose() {
    folder.dispose();
    isEdit.dispose();
    initialize.dispose();
    submit.dispose();
  }
}
