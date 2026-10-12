import 'dart:io' show File;

import 'package:image_picker/image_picker.dart';

/// Picks media from the device.
///
/// A feature states what it wants — "an image, from the gallery" — rather
/// than which plugin delivers it, and a cancelled pick is one nullable answer
/// instead of a platform-specific exception. A refusal (a denied permission, a
/// platform that cannot open its picker) is a real failure and is left to the
/// caller to report: it is the screen that knows what the picture was for.
final class MediaService {
  /// Creates a service over a picker.
  new({ImagePicker? picker}) : _picker = picker ?? ImagePicker();

  final ImagePicker _picker;

  /// Asks the user for an image, or answers null when they back out.
  ///
  /// [fromGallery] picks the local library; otherwise the camera is opened.
  Future<File?> getImage({bool fromGallery = true}) async {
    final picked = await _picker.pickImage(
      source: fromGallery ? ImageSource.gallery : ImageSource.camera,
    );
    return picked == null ? null : File(picked.path);
  }
}
