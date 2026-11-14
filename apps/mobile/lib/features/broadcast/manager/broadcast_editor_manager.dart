import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/broadcast/manager/active_broadcast_manager.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';
import 'package:meno_mobile/features/broadcast/services/_services.dart';
import 'package:meno_mobile/features/profile/model/_model.dart';
import 'package:uuid/uuid.dart';

/// The form that writes a broadcast, and the draft behind it.
///
/// Creating a broadcast is four steps the API has no single call for — create
/// the row, store its cover, go live for a token, hand the session to the live
/// owner — and the draft records how far it got, so a creation interrupted
/// between two of them is resumed rather than started again. A draft is written
/// on every edit for the
/// same reason: the form is the only place the user's work lives until the API
/// has it.
final class BroadcastEditorManager implements Disposable {
  new({
    required this._http,
    required this._local,
    required this._media,
    required this._active,
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
        try {
          if (_createdBroadcastId != null &&
              step.value.index >= BroadcastCreationStep.created.index) {
            // The row exists but its cover may not: an earlier press can have
            // stopped between the two, and going live now would leave the
            // picture the user picked on this device alone.
            await _uploadArtwork();
            return;
          }

          final created = await _http.create(
            // The draft's own key: a retry of this creation is the same logical
            // operation, so the API replays what it already did instead of
            // making a second broadcast (audit H20).
            idempotencyKey: draft.idempotencyKey.isEmpty
                ? (_createKey ??= _uuid.v4())
                : draft.idempotencyKey,
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

          // Before the chain goes on to `startBroadcast`: the cover is stored
          // under the broadcast's id, and the API refuses to change the cover
          // of a row that is already on air.
          await _uploadArtwork();
        } on Object {
          // A creation that did not finish is the one time a draft is kept
          // without being asked for.
          await _keepFailedStart();
          rethrow;
        }
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'broadcast.editor.create',
    );

    startBroadcast = .createAsyncNoParamNoResult(
      () async {
        final broadcastId = _createdBroadcastId;
        if (broadcastId == null) return;

        // Already in the room: a second press is not a second start.
        if (session.value != null) return;

        _isRunning.value = true;
        try {
          // Where the chain resumes from. `.started` is the *server's* commit,
          // recorded as soon as the API answers — so an entry that fails, or a
          // process that dies, leaves a form that knows the broadcast exists.
          final committed =
              step.value.index >= BroadcastCreationStep.started.index
              ? null
              : await _goLive(broadcastId);

          if (step.value.index < BroadcastCreationStep.started.index) {
            step.value = .started;
            await _persistCreationState(broadcastId, step.value);
          }

          // One API call for one go-live: the session obtained above is
          // attached rather than fetched a second time. When there is none to
          // attach, the broadcast already committed and `resume` re-mints the
          // token — `goLive` refuses a broadcast that is on air.
          final entry = await _active.enter(
            LiveEnterRequest(
              broadcastId: broadcastId,
              mode: committed == null ? .resume : .host,
              session: committed,
            ),
          );
          if (!entry.isEntered) {
            // On air but not in the room: a start that did not finish, which is
            // the other case a draft is kept for. The step is `started`, so the
            // retry asks only for the entry.
            await _keepFailedStart();
            return;
          }

          session.value = committed ?? _active.session?.details;
          await _forgetStartedDraft();
        } on Object {
          await _keepFailedStart();
          rethrow;
        } finally {
          _isRunning.value = false;
        }
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
      pickedCohosts.value = [];
      record.value = false;
      session.value = null;
      _draftId = null;
      _createKey = null;
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
      // The stored draft kept the ids and nothing else, so a reopened form
      // shows its chips without names.
      pickedCohosts.value = [];

      _createdBroadcastId = draft.createdBroadcastId;
      step.value = draft.creationStep;
      _createKey = draft.idempotencyKey.isEmpty ? null : draft.idempotencyKey;
      _liveKey = null;

      // What of this picture the API already has, if any: resuming a creation
      // must not upload it a second time, but a new pick still must.
      _uploadedImageId = draft.imageId;
      _uploadedImagePath = draft.imageId == null
          ? null
          : draft.imagePath?.getFile?.path;
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
  final ActiveBroadcastManager _active;
  final Id _userId;
  final MenoLogger _logger;

  /// Creates the keys the API recognises a replayed mutation by.
  final Uuid _uuid = const Uuid();

  /// The draft being edited, or null while the form is new.
  Id? _draftId;

  /// The key this draft's creation is sent with, before it is written down.
  ///
  /// Kept here as well as on the draft because the first save is what stores
  /// it: generating a new one at send time would make a retry a second create.
  String? _createKey;

  /// The key this attempt at going on air is sent with.
  ///
  /// Per editor session rather than per draft: nothing is created by going
  /// live, so a restart is a genuinely new operation, while pressing Start
  /// again after a lost response is the same one.
  String? _liveKey;

  /// The broadcast that was created by this draft, if any.
  Id? _createdBroadcastId;

  /// The key the API stored the chosen cover under, as the draft last knew it.
  String? _uploadedImageId;

  /// The local path whose bytes that key holds.
  ///
  /// Kept beside [imagePath] so a second press of Start does not send the same
  /// picture again, and so a *different* picture picked afterwards still does.
  String? _uploadedImagePath;

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

  /// The accounts behind [cohosts], when the picker named them.
  ///
  /// A draft stores ids, and a draft reopened after a restart has no names to
  /// show; the chips fall back to a plain label rather than inventing one.
  final pickedCohosts = ValueNotifier<List<Profile>>([]);
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
  /// Capped at [maxCohosts], which is the API's own limit: an invitation past
  /// it would be refused at creation, so the form stops where the API does.
  void toggleCohost(Id cohostId, {Profile? profile}) {
    hasUnsavedChanges.value = true;
    if (cohosts.contains(cohostId)) {
      cohosts.remove(cohostId);
      pickedCohosts.value = [
        for (final item in pickedCohosts.value)
          if (item.id != cohostId) item,
      ];
      return;
    }
    if (cohosts.length >= maxCohosts) return;
    cohosts.add(cohostId);
    if (profile != null) {
      pickedCohosts.value = [...pickedCohosts.value, profile];
    }
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

  /// `POST /broadcasts/{id}/live`, then the entry that opens the room.
  ///
  /// The session is the only thing that carries a room token, so going on air
  /// is also what opens the session: the live owner creates the transport, the
  /// roster and the clock, and the screen that navigates to the room finds the
  /// snapshot already live.
  late final Command<void, void> startBroadcast;

  /// Empties the form and forgets the draft it was editing.
  late final Command<void, void> resetForm;

  /// Opens `draft` in the form.
  late final Command<BroadcastDraft, void> selectDraft;

  /// Forgets draftId`` and re-reads the list.
  late final Command<Id, void> deleteDraft;

  /// The form as a draft, in memory only.
  ///
  /// Starting a broadcast is not a reason to write a draft: the creation the
  /// user asked for is the API's, and a device copy of it afterwards is work
  /// they never asked to keep. Only two things write one — the exit prompt's
  /// "save", and a creation that did not finish ([_keepFailedStart]) — and the
  /// id is minted here so both of those can share the object the press built.
  BroadcastDraft buildDraft() => BroadcastDraft(
    id: _draftId ?? Id.unique(),
    ownerId: _userId,
    title: title.value,
    description: description.value,
    record: record.value,
    cohosts: cohosts.toList(),
    imagePath: imagePath.value,
    imageId: _uploadedImageId,
    createdBroadcastId: _createdBroadcastId,
    creationStep: step.value,
    idempotencyKey: _createKey ??= _uuid.v4(),
    lastModified: DateTime.now(),
  );

  /// The form as a draft, written to the device.
  ///
  /// Called by the exit prompt when the user asks to keep the work, and by a
  /// failed creation so its retry can resume instead of start again.
  Future<BroadcastDraft> saveDraft() async {
    final draft = buildDraft();

    if (draft.isEmpty) return draft;

    _draftId = draft.id;
    await _local.saveDraft(draft);
    hasUnsavedChanges.value = false;
    _loadDrafts();
    return draft;
  }

  /// Keeps the work when a start could not finish.
  ///
  /// The one automatic draft. A creation that stopped half-way is the case the
  /// draft exists for: `createdBroadcastId` and `creationStep` are what make
  /// the retry pick up at the phase that failed. A failure saving it must not
  /// replace the failure the user is about to be shown, so this is quiet.
  Future<void> _keepFailedStart() async {
    try {
      await saveDraft();
    } on Object catch (error) {
      _logger.w('could not keep the draft after a failed start', error: error);
    }
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
    pickedCohosts.dispose();
    record.dispose();
    session.dispose();
    _isRunning.dispose();
  }

  /// How many co-hosts a broadcast may have — `MAX_COHOSTS` in the API.
  static const int maxCohosts = 3;

  void _loadDrafts() {
    final stored = _local.drafts();
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
    _uploadedImageId = null;
    _uploadedImagePath = null;
    // The next creation is a different broadcast, so it must not be able to
    // replay this one's stored outcome.
    _liveKey = null;
    // Unconditionally: a form that has moved on must not be able to point the
    // next creation at the row it just finished. `step` alone was not enough —
    // a start whose entry failed leaves the id behind with the step still
    // unset, and that is exactly the state that restarts an ended broadcast.
    _createdBroadcastId = null;
    step.value = BroadcastCreationStep.none;
  }

  /// Goes on air, treating "already live" as the commit it is.
  ///
  /// A lost response, a second press and a client that never saw the first
  /// answer all arrive here as `ALREADY_LIVE` — and the broadcast *is* on air,
  /// so the only step left is the entry. Returning `null` says "nothing new to
  /// attach", which the caller turns into a resume instead of an error the
  /// user cannot act on.
  Future<BroadcastSession?> _goLive(Id broadcastId) async {
    // One key for every retry of this start: going on air is the logical
    // operation, not the press.
    final idempotencyKey = _liveKey ??= _uuid.v4();

    try {
      return (await _http.goLive(
        broadcastId,
        idempotencyKey: idempotencyKey,
      )).toDomain(_userId);
    } on MenoException catch (error) {
      if (error.code == MenoErrorCode.alreadyLive) return null;
      rethrow;
    }
  }

  /// Forgets the draft a started broadcast came from.
  ///
  /// A draft that outlives its broadcast is a button that restarts an ended
  /// one: its `createdBroadcastId` names a terminal row, so every later press
  /// is refused and the user is held on a form they already finished. The
  /// broadcast itself lives on in the catalogue and the ended page; the
  /// draft's job is done.
  Future<void> _forgetStartedDraft() async {
    final draftId = _draftId;
    // Cleared first, so nothing written after this point re-creates the row.
    _draftId = null;
    if (draftId == null) return;

    await _local.deleteDraft(draftId);
    _loadDrafts();
  }

  /// Sends the picked picture to the API, once.
  ///
  /// The editor keeps artwork on the device; this is what turns it into the
  /// `imageUrl` every card and the room's own header read. It happens between
  /// creating the row and going live, because `POST /broadcasts/{id}/image`
  /// needs the row to exist and refuses one that is already on air — and a
  /// failure here stops the chain before the broadcast is on air, so pressing
  /// Start again retries it rather than going on air without a cover.
  Future<void> _uploadArtwork() async {
    // Past the commit the row is on air, and the API refuses a cover change on
    // a broadcast that is live. A resumed draft whose picture never reached the
    // server keeps the one it has rather than failing the chain it is meant to
    // finish.
    if (step.value.index >= BroadcastCreationStep.started.index) return;

    final broadcastId = _createdBroadcastId;
    final file = imagePath.value?.getFile;
    if (broadcastId == null || file == null) return;
    if (_uploadedImagePath == file.path) return;

    final uploaded = await _http.uploadImage(broadcastId, file);
    _uploadedImageId = uploaded.imageId;
    _uploadedImagePath = file.path;
    await _persistCreationState(
      broadcastId,
      step.value,
      imageId: uploaded.imageId,
    );
  }

  Future<void> _persistCreationState(
    Id broadcastId,
    BroadcastCreationStep currentStep, {
    String? imageId,
  }) async {
    final draftId = _draftId;
    if (draftId == null) return;

    final existing = _local.draft(draftId);
    if (existing == null) return;

    await _local.saveDraft(
      existing.copyWith(
        createdBroadcastId: broadcastId,
        creationStep: currentStep,
        imagePath: imagePath.value,
        imageId: imageId ?? existing.imageId,
      ),
    );
    _loadDrafts();
  }
}
