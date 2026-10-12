import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno/_core/_core.dart';
import 'package:meno/_shared/_shared.dart';
import 'package:meno/features/profile/manager/_manager.dart';
import 'package:meno/features/profile/model/_model.dart';
import 'package:meno/features/profile/services/_services.dart';

class MyProfileProxy extends ChangeNotifier implements Disposable {
  MyProfileProxy(this._profile)
    : _fullName = ValueNotifier(_profile.fullName),
      _bio = ValueNotifier(_profile.bio),
      _image = ValueNotifier(ImageInput.empty);

  Profile _profile;

  Profile get profile => _profile;

  set profile(Profile value) {
    _profile = value;
    notifyListeners();
  }

  final ValueNotifier<SingleLineString> _fullName;
  final ValueNotifier<MultiLineString?> _bio;
  final ValueNotifier<ImageInput?> _image;

  SingleLineString get fullName => _fullName.value;

  MultiLineString? get bio => _bio.value;

  ImageInput? get image => _image.value;

  void onNameChanged(String v) => _fullName.value = SingleLineString(v);

  void onBioChanged(String v) => _bio.value = MultiLineString(v);

  Future<void> onImageChanged([bool fromGallery = true]) async {
    final file = await di<MediaService>().getImage(fromGallery: fromGallery);
    if (file != null) _image.value = ImageInput.fromFile(file);
  }

  void onImageRemoved() => _image.value = null;

  late final isFormValid = _fullName.combineLatest3(
    _bio,
    _image,
    (a, b, c) => a.isValid && (b?.isValid ?? true) && (c?.isValid ?? true),
  );

  late final hasChanges = _fullName.combineLatest3(
    _bio,
    _image,
    (a, b, c) => (a != _profile.fullName) || (b != _profile.bio) || c != null,
  );

  late final updateProfile = Command.createAsyncNoParamNoResult(() async {
    final userId = di<UserManager>().currentUserId.value;
    final updated = await di<ProfileHttpService>().editProfile(
      userId: userId,
      fullName: _fullName.value,
      bio: _bio.value,
      image: _image.value,
    );

    di<MyProfileManager>().updateProfile(updated);
    _profile = updated;
    notifyListeners();

    unawaited(di<ProfileLocalService>().cacheProfile(userId, updated));
  }, errorFilterFn: menoExceptionFilter);

  @override
  FutureOr<dynamic> onDispose() {
    _fullName.dispose();
    _bio.dispose();
    _image.dispose();
    updateProfile.dispose();
    super.dispose();
  }
}
