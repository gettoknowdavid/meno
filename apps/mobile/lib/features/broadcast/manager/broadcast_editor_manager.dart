import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/broadcast/manager/live_scope_manager.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';
import 'package:meno_mobile/features/broadcast/services/_services.dart';

/// The form that writes a broadcast, and the draft behind it.
///
/// Creating a broadcast is three steps the API has no single call for — create
/// the row, go live for a token, hand the session to the room — and the draft
/// records how far it got, so a creation interrupted between two of them is
/// resumed rather than started again. A draft is written on every edit for the
/// same reason: the form is the only place the user's work lives until the API
/// has it.
final class BroadcastEditorManager implements Disposable {
  new({
    required this._http,
    required this._local,
    required this._media,
    required this._live,
    required this._userId,
    required this._logger,
  }) {
    loadDrafts = .createSyncNoParamNoResult(
      _loadDrafts,
      errorFilterFn: menoExceptionFilter,
      debugName: 'broadcast.editor.loadDrafts',
    );

    createBroadcast = .createAsyncNoResult<BroadcastDraft>(
      (draft) async {
        if (_createdBroadcastId != null &&
            step.value.index >= BroadcastCreationStep.created.index) {
          return;
        }

        final created = await _http.create(
          title: draft.title,
          description: (draft.description?.isEmpty ?? false)
              ? null
              : draft.description,
          imageId: draft.imageId,
          cohosts: [for (final id in draft.cohosts) id.getOrCrash],
          recordingEnabled: draft.record,
        );
        _createdBroadcastId = created.id;
        step.value = .created;
        await _persistCreationState(created.id, step.value);
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'broadcast.editor.create',
    );

    startBroadcast = .createAsyncNoParamNoResult(
      () async {
        final broadcastId = _createdBroadcastId;
        if (broadcastId == null) return;
        if (step.value.index >= BroadcastCreationStep.started.index) return;

        _isRunning.value = true;
        BroadcastSession created;
        try {
          created = (await _http.goLive(broadcastId)).toDomain(_userId);
        } finally {
          _isRunning.value = false;
        }
        session.value = created;
        step.value = .started;
        await _persistCreationState(broadcastId, step.value);
        await _live.start(broadcastId, session: created);
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'broadcast.editor.start',
    );

    resetForm = .createSyncNoParamNoResult(() {
      hasUnsavedChanges.value = false;
      title.value = '';
      description.value = '';
      imagePath.value = null;
      cohosts.startTransAction();
      cohosts.clear();
      cohosts.endTransAction();
      record.value = false;
      session.value = null;
      _draftId = null;
      _resetCreationState();
    });

    selectDraft = .createSyncNoResult<BroadcastDraft>((draft) {
      // Opening a draft is not an edit: leaving an untouched one must not ask
      // whether to save it again.
      hasUnsavedChanges.value = false;
      _draftId = draft.id;
      title.value = draft.title;
      description.value = draft.description;
      imagePath.value = draft.imagePath;
      record.value = draft.record;
      cohosts.startTransAction();
      cohosts.clear();
      cohosts.addAll(draft.cohosts);
      cohosts.endTransAction();

      _createdBroadcastId = draft.createdBroadcastId;
      step.value = draft.creationStep;
    });

    deleteDraft = .createAsyncNoResult<Id>(
      (draftId) async {
        await _local.deleteDraft(draftId);
        if (_draftId == draftId) resetForm.run();
        _loadDrafts();
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'broadcast.editor.deleteDraft',
    );

    // Read once at construction: the drafts are already on the device, and a
    // list that is only populated after the first save is a "Drafts" button
    // that shows nothing the first time it is pressed.
    _loadDrafts();
  }

  final BroadcastHttpService _http;
  final BroadcastLocalService _local;
  final MediaService _media;
  final LiveScopeManager _live;
  final Id _userId;
  final MenoLogger _logger;

  /// The draft being edited, or null while the form is new.
  Id? _draftId;

  /// The broadcast that was created by this draft, if any.
  Id? _createdBroadcastId;

  /// The account's drafts, newest edit first.
  final drafts = ListNotifier<BroadcastDraft>(data: []);

  /// How far the creation this draft describes has got.
  final ValueNotifier<BroadcastCreationStep> step = ValueNotifier(.none);

  /// The title as typed, so half-typed or invalid text survives a save.
  final ValueNotifier<String> title = ValueNotifier('');

  /// The description as typed, for the same reason.
  final ValueNotifier<String?> description = ValueNotifier('');

  /// The locally picked image, until the API has one.
  final imagePath = ValueNotifier<ImageInput?>(null);

  final cohosts = ListNotifier<Id>(data: []);
  final ValueNotifier<bool> record = ValueNotifier(false);

  /// The session going live produced, once there is one.
  final session = ValueNotifier<BroadcastSession?>(null);

  /// Whether the form can be submitted.
  ///
  /// A title and a description are what the API requires; the cohost limit is
  /// the editor's own rule and is enforced by [toggleCohost].
  late final ValueListenable<bool> canSubmit = title.map(
    (value) => SingleLineString(value).isValid,
  );

  void onTitleChanged(String input) {
    hasUnsavedChanges.value = true;
    title.value = input;
  }

  void onDescriptionChanged(String input) {
    hasUnsavedChanges.value = true;
    description.value = input;
  }

  void onToggleRecord(bool input) {
    hasUnsavedChanges.value = true;
    record.value = input;
  }

  /// Adds [cohostId] to the form, or removes it.
  ///
  /// Capped at two: the API accepts more, but the pre-stream lobby and the
  /// broadcast card both draw slots for two, so the form stops at what can be
  /// seen.
  void toggleCohost(Id cohostId) {
    hasUnsavedChanges.value = true;
    if (cohosts.contains(cohostId)) {
      cohosts.remove(cohostId);
      return;
    }
    if (cohosts.length >= _maxCohosts) return;
    cohosts.add(cohostId);
  }

  /// Picks an image with the platform picker.
  Future<void> pickImage({bool fromGallery = true}) async {
    final file = await _media.getImage(fromGallery: fromGallery);
    if (file == null) return;
    hasUnsavedChanges.value = true;
    imagePath.value = ImageInput.fromFile(file);
  }

  void removeImage() {
    hasUnsavedChanges.value = true;
    imagePath.value = null;
  }

  /// Loads the account's drafts.
  late final Command<void, void> loadDrafts;

  /// `POST /broadcasts`, resuming a half-made one when there is one.
  late final Command<BroadcastDraft, void> createBroadcast;

  /// `POST /broadcasts/{id}/live`, answering with the session to enter with.
  ///
  /// The session is the only thing that carries a room token, so going on air
  /// is also what opens the live scope: from here the LiveKit client, the
  /// session manager and the room's other services exist, and the screen that
  /// navigates to the room finds them already there.
  late final Command<void, void> startBroadcast;

  /// Empties the form and forgets the draft it was editing.
  late final Command<void, void> resetForm;

  /// Opens `draft` in the form.
  late final Command<BroadcastDraft, void> selectDraft;

  /// Forgets draftId`` and re-reads the list.
  late final Command<Id, void> deleteDraft;

  /// The form as a draft, writing it to the device first.
  Future<BroadcastDraft> saveDraft() async {
    final draft = BroadcastDraft(
      id: _draftId ??= Id.unique(),
      ownerId: _userId,
      title: title.value,
      description: description.value,
      record: record.value,
      cohosts: cohosts.toList(),
      imagePath: imagePath.value,
      createdBroadcastId: _createdBroadcastId,
      creationStep: step.value,
      lastModified: DateTime.now(),
    );

    if (draft.isEmpty) return draft;

    await _local.saveDraft(draft);
    hasUnsavedChanges.value = false;
    _loadDrafts();
    return draft;
  }

  /// Whether the form holds something the device has not been given.
  ///
  /// Set by every field that changes, cleared by saving and by opening a saved
  /// draft. The editor reads it to decide whether leaving is worth a question:
  /// asking "save or discard?" over an untouched form is a dialog about
  /// nothing.
  final ValueNotifier<bool> hasUnsavedChanges = ValueNotifier(false);

  /// Throws the form away, deleting its device copy when there is one.
  ///
  /// "Discard" in the exit prompt means the user does not want what they typed.
  /// A form opened from a saved draft still has that row, so keeping it would
  /// offer them the work they just threw away the next time they open the
  /// editor.
  Future<void> discardDraft() async {
    final draftId = _draftId;
    resetForm.run();
    if (draftId != null) await deleteDraft.runAsync(draftId);
  }

  /// Whether a step of the creation is in flight.
  ValueListenable<bool> get isRunning => _isRunning;
  final ValueNotifier<bool> _isRunning = ValueNotifier(false);

  @override
  FutureOr<dynamic> onDispose() {
    createBroadcast.dispose();
    startBroadcast.dispose();
    resetForm.dispose();
    selectDraft.dispose();
    deleteDraft.dispose();
    loadDrafts.dispose();

    step.dispose();
    hasUnsavedChanges.dispose();
    drafts.dispose();
    title.dispose();
    description.dispose();
    imagePath.dispose();
    cohosts.dispose();
    record.dispose();
    session.dispose();
    _isRunning.dispose();
  }

  static const int _maxCohosts = 2;

  void _loadDrafts() {
    final stored = _local.drafts(_userId);
    drafts.startTransAction();
    drafts.clear();
    drafts.addAll(stored);
    drafts.endTransAction();

    _logger.d(
      'Reading ${stored.length} draft broadcasts from the device',
      tag: MenoLogTag.lifecycle.label,
    );
  }

  void _resetCreationState() {
    if (step.value.index >= BroadcastCreationStep.created.index) {
      _createdBroadcastId = null;
      step.value = BroadcastCreationStep.none;
    }
  }

  Future<void> _persistCreationState(
    Id broadcastId,
    BroadcastCreationStep currentStep,
  ) async {
    final draftId = _draftId;
    if (draftId == null) return;

    final existing = _local.draft(draftId);
    if (existing == null) return;

    await _local.saveDraft(
      existing.copyWith(
        createdBroadcastId: broadcastId,
        creationStep: currentStep,
        imagePath: imagePath.value,
        imageId: existing.imageId,
      ),
    );
    _loadDrafts();
  }
}
