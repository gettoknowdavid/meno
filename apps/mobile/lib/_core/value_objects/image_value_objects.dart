import 'dart:io' show File;

import 'package:equatable/equatable.dart';
import 'package:fpdart/fpdart.dart' show Either, Left, Right;
import 'package:meno_mobile/_core/value_objects/_value_objects.dart';

sealed class ImageOrigin with Equatable {
  const new();
}

final class NetworkImageOrigin extends ImageOrigin {
  const new(this.url);

  final String url;

  @override
  List<Object?> get props => [url];
}

final class LocalImageOrigin extends ImageOrigin {
  const new(this.file);

  final File file;

  @override
  List<Object?> get props => [file];
}

class ImageInput extends ValueObject<ImageOrigin?> {
  factory fromFile(File? file) {
    if (file == null) return ImageInput.empty;
    return ImageInput._(_validateFile(file));
  }

  factory fromUrl(String? url) {
    if (url == null || url.trim().isEmpty) return ImageInput.empty;
    return ImageInput._(_validateUrl(url));
  }

  const new _(super.value);

  static const ImageInput empty = ImageInput._(Right(null));

  static Either<ValueException<ImageOrigin?>, ImageOrigin?> _validateFile(
    File file,
  ) {
    final extension = file.path.split('.').last.toLowerCase();
    const allowed = ['jpg', 'jpeg', 'png', 'webp', 'heic', 'heif'];

    if (!allowed.contains(extension)) {
      return Left(
        InvalidFileFormatException<ImageOrigin?>(
          LocalImageOrigin(file),
          msg: 'Invalid file format.',
        ),
      );
    }
    return Right(LocalImageOrigin(file));
  }

  static Either<ValueException<ImageOrigin?>, ImageOrigin?> _validateUrl(
    String url,
  ) {
    final uri = Uri.tryParse(url);
    if (uri == null || !uri.hasScheme) {
      return Left(
        InvalidValueException<ImageOrigin?>(
          NetworkImageOrigin(url),
          msg: 'Invalid URL format.',
        ),
      );
    }
    return Right(NetworkImageOrigin(url));
  }
}

extension ImageInputX on ImageInput {
  String? get getUrl => (getOrNull as NetworkImageOrigin?)?.url;

  File? get getFile => (getOrNull as LocalImageOrigin?)?.file;
}
