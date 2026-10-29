import 'dart:async';
import 'dart:io' show File;

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/profile/manager/my_profile_manager.dart';
import 'package:meno_mobile/features/profile/model/_model.dart';
import 'package:meno_mobile/features/profile/services/profile_http_service.dart';

/// The edit-profile form.
///
/// Created by the scope the editor sheet pushes, so the fields live exactly as
/// long as the sheet does: two visits cannot share a half-typed name, and there
/// is nothing to remember to clear. What it saves goes straight to
/// [MyProfileManager], which keeps the profile screen, the cache and the
/// sheet's own starting point in step.
final class ProfileEditorManager implements Disposable {
  /// Creates the form for [profile].
  new({required MyProfile profile, required this._http, required this._owner})
    : _originalName = profile.fullName.orEmpty,
      _originalBio = profile.bio?.getOrNull ?? '',
      _original = profile,
      fullName = ValueNotifier(profile.fullName),
      bio = ValueNotifier(profile.bio);

  /// The longest bio the API accepts (`BIO_MAX` in the profile DTO).
  static const int bioMax = 244;

  final MyProfile _original;
  final ProfileHttpService _http;
  final MyProfileManager _owner;

  /// The name as it was when the form opened, unwrapped.
  ///
  /// Plain text on purpose: the dirty check below compares like with like. The
  /// audit's defect was comparing a `String` with a value object, which made an
  /// untouched field look edited (M11).
  final String _originalName;

  /// The bio as it was when the form opened, unwrapped — the raw text, not the
  /// value object.
  final String _originalBio;

  /// Whether the sheet has been disposed.
  ///
  /// Disposing a command does not cancel its body, so the save checks this
  /// after every await: an upload that finishes after the sheet closed must not
  /// be followed by a patch (audit H15).
  bool _disposed = false;

  /// The name field.
  ///
  /// A [FullName], with the same policy the field always had: this is an
  /// existing display name being edited, and re-typing a name the API already
  /// accepted must pass. The API's own length rule arrives as a validation
  /// failure naming the field.
  final ValueNotifier<FullName> fullName;

  /// The bio field, or null when it is to be removed.
  ///
  /// Typing and then clearing the field is how a bio is deleted: the API wants
  /// an empty string for that, and a null here means "send nothing".
  final ValueNotifier<MultiLineString?> bio;

  /// The avatar picked on this visit, if any.
  final ValueNotifier<ImageInput> avatar = ValueNotifier(ImageInput.empty);

  /// The avatar the account has right now, before anything picked here.
  ImageInput? get currentAvatarUrl => _original.avatarUrl;

  /// Whether every field currently holds something acceptable.
  ///
  /// The bio counts: an over-long bio is invalid, and invalid must mean "save
  /// is disabled", not "send an empty string and delete it" — which is exactly
  /// how a bad comment used to turn a typo into a deletion (M11). An empty bio
  /// is valid: clearing the field is how the account removes it.
  late final ValueListenable<bool> isValid = fullName.combineLatest(
    bio,
    (name, bioValue) => name.isValid && (bioValue == null || bioValue.isValid),
  );

  /// Whether the form would change anything.
  ///
  /// The save button follows this, because a patch carrying neither a name nor
  /// a bio is refused by the API, and a save that saves nothing is worse than
  /// no button.
  late final ValueListenable<bool> hasChanges = fullName.combineLatest3(
    bio,
    avatar,
    (name, bioValue, image) =>
        name.orEmpty.trim() != _originalName.trim() ||
        (bioValue?.getOrNull ?? '').trim() != _originalBio.trim() ||
        image.getFile != null,
  );

  /// Writes the form to the API and adopts the answer.
  late final Command<void, MyProfile> save =
      Command.createAsyncNoParam<MyProfile>(
        _save,
        initialValue: MyProfile.empty,
        errorFilterFn: menoExceptionFilter,
        debugName: 'profile.save',
      );

  /// Records a new name.
  void onFullNameChanged(String value) => fullName.value = FullName(value);

  /// Records a new bio, or its removal when the field is emptied.
  void onBioChanged(String value) {
    final clean = value.trim();
    bio.value = clean.isEmpty
        ? null
        : MultiLineString(clean, maxLength: bioMax);
  }

  /// Records a new avatar, picked from the gallery or the camera.
  void onAvatarChanged(File? file) => avatar.value = ImageInput.fromFile(file);

  @override
  FutureOr<dynamic> onDispose() {
    _disposed = true;
    fullName.dispose();
    bio.dispose();
    avatar.dispose();
    save.dispose();
  }

  /// Uploads the avatar, then patches what changed, then adopts the result.
  ///
  /// Two requests at most, and only for the parts that moved. The avatar goes
  /// first because `PATCH /users/me` answers with the fresh record — including
  /// the new `avatar_url` — so the last answer already carries both changes
  /// and one of them cannot overwrite the other.
  Future<MyProfile> _save() async {
    var profile = _owner.profile.value;

    final file = avatar.value.getFile;
    if (file != null) {
      final uploaded = await _http.uploadAvatar(file);
      // The sheet, or the account behind it, may be gone by the time the
      // upload lands; patching then would write to somebody else's session.
      if (_disposed) return profile;

      // The URL is applied when the deployment publishes one; without a
      // public base URL the API answers null and the key alone is all there is.
      profile = profile.copyWith(
        avatarId: uploaded.id,
        avatarUrl: ImageInput.fromUrl(uploaded.url),
      );
    }

    if (_disposed) return profile;

    final name = fullName.value.orEmpty;
    // Only what moved travels: a field left alone is null, and an emptied bio
    // is the empty string the API reads as "remove it".
    final bioValue = bio.value?.getOrNull;
    final bioField = (bioValue ?? '') == _originalBio ? null : (bioValue ?? '');

    if (name == _originalName) {
      if (bioField != null) profile = await _http.updateMe(bio: bioField);
    } else {
      profile = await _http.updateMe(fullName: name, bio: bioField);
    }

    // The answer belongs to the moment it was asked in; a sheet that closed
    // meanwhile writes nothing.
    if (_disposed) return profile;

    _owner.update(profile);
    return profile;
  }
}
