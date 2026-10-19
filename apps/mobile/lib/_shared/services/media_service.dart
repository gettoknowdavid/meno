import 'dart:io' show File;

import 'package:flutter/services.dart' show PlatformException;
import 'package:image_picker/image_picker.dart';

/// Picks media from the device.
///
/// A feature states what it wants — "an image, from the gallery" — rather
/// than which plugin delivers it, and a cancelled pick is one nullable answer
/// instead of a platform-specific exception.
///
/// # No permission is requested up front
///
/// The gallery opens the system's own photo picker, which hands over only what
/// the user chooses and needs no permission, and the camera opens the system
/// camera. Asking for either through a permissions plugin first would be an
/// extra prompt for nothing — and on Android it would come back denied, since
/// the manifest deliberately does not declare `CAMERA`. The one refusal worth
/// handling is an iOS user who said no to the camera prompt: that is reported
/// as [MediaAccessDenied], so the screen can send them to Settings. Any other
/// failure is a real one and is left to the caller to report.
final class MediaService {
  /// Creates a service over a picker.
  new({ImagePicker? picker}) : _picker = picker ?? ImagePicker();

  final ImagePicker _picker;

  static const String _cameraDenied = 'camera_access_denied';
  static const String _photosDenied = 'photo_access_denied';

  /// Asks the user for an image, or answers null when they back out.
  ///
  /// [fromGallery] picks the local library; otherwise the camera is opened.
  ///
  /// Throws [MediaAccessDenied] when the platform refuses because the user
  /// turned the camera (or photo) access off for the app.
  Future<File?> getImage({bool fromGallery = true}) async {
    try {
      final picked = await _picker.pickImage(
        source: fromGallery ? ImageSource.gallery : ImageSource.camera,
        // Full metadata (location, EXIF) is what makes iOS ask for photo
        // library access. An artwork upload has no use for it.
        requestFullMetadata: false,
      );
      return picked == null ? null : File(picked.path);
    } on PlatformException catch (error) {
      if (error.code == _cameraDenied) {
        throw const MediaAccessDenied(camera: true);
      }
      if (error.code == _photosDenied) {
        throw const MediaAccessDenied(camera: false);
      }
      rethrow;
    }
  }
}

/// The user turned the camera or the photo library off for the app.
///
/// Only the system settings can undo that, which is what makes it different
/// from a picker that failed to open.
final class MediaAccessDenied implements Exception {
  /// Creates the refusal; [camera] says which of the two was refused.
  const new({required this.camera});

  /// Whether the camera was refused, rather than the photo library.
  final bool camera;

  @override
  String toString() => 'MediaAccessDenied(${camera ? 'camera' : 'photos'})';
}
