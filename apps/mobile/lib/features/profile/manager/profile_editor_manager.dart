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
    : _original = profile,
      fullName = ValueNotifier(profile.fullName),
      bio = ValueNotifier(profile.bio);

  /// The longest bio the API accepts (`BIO_MAX` in the profile DTO).
  static const int bioMax = 244;

  final MyProfile _original;
  final ProfileHttpService _http;
  final MyProfileManager _owner;

  /// The name field.
  ///
  /// A single line rather than a [FullName]: this is an existing display name
  /// being edited, not a new registration, and the two have different rules —
  /// re-typing a name the API already accepted must not fail the app's own
  /// stricter signup validation. The length rule the API does apply arrives as
  /// a validation failure naming the field.
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
  late final ValueListenable<bool> isValid = fullName.select(
    (name) => name.isValid,
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
        name != _original.fullName ||
        (bioValue?.getOrNull ?? '') != (_original.bio ?? '') ||
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
      // The URL is applied when the deployment publishes one; without a
      // public base URL the API answers null and the key alone is all there is.
      profile = profile.copyWith(
        avatarId: uploaded.id,
        avatarUrl: ImageInput.fromUrl(uploaded.url),
      );
    }

    final name = fullName.value.orEmpty;
    // Only what moved travels: a field left alone is null, and an emptied bio
    // is the empty string the API reads as "remove it".
    final bioValue = bio.value?.getOrNull;
    final bioField = (bioValue ?? '') == (_original.bio ?? '')
        ? null
        : (bioValue ?? '');

    if (name == _original.fullName.orEmpty) {
      if (bioField != null) profile = await _http.updateMe(bio: bioField);
    } else {
      profile = await _http.updateMe(fullName: name, bio: bioField);
    }

    _owner.update(profile);
    return profile;
  }
}
