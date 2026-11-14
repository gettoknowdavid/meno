import 'dart:io';

import 'package:cached_network_image/cached_network_image.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/placeholders/m_placeholder.dart';
import 'package:meno_ui/src/theme/extensions/m_color_scheme.dart';
import 'package:meno_ui/src/tokens/strokes.dart';
import 'package:skeletonizer/skeletonizer.dart';

/// The longest edge, in pixels, that a network avatar is decoded at.
///
/// Avatars are drawn small, so decoding a thumbnail keeps the image cache from
/// filling up with full-resolution photos.
const int _kDecodeSize = 512;

/// A circular avatar built from a network URL, a local file or a child widget.
///
/// The ring around the avatar is drawn from the theme, and only one image
/// source is ever used: a [file] wins over a [url], and a [child] stands in
/// when neither is available.
///
/// Example usage:
/// ```dart
/// MAvatar(
///   radius: 30,
///   url: 'https://example.com/avatar.png',
///   onTap: openProfile,
/// )
/// ```
class MAvatar extends StatelessWidget {
  /// Creates an [MAvatar] widget.
  ///
  /// The [radius] parameter is required and sets the size of the circle. The
  /// [url] and [file] parameters are the two image sources, [child] is the
  /// fallback content, and [onTap] handles taps. The [hasBorder] parameter
  /// keeps the ring around the avatar, and [loading] suppresses the image while
  /// the source is being fetched.
  const new({
    required this.radius,
    super.key,
    this.url,
    this.file,
    this.child,
    this.onTap,
    this.hasBorder = true,
    this.loading = false,
  });

  /// The radius of the avatar.
  ///
  /// This determines the size of the circular avatar.
  final double radius;

  /// The URL of the background image for the avatar.
  final String? url;

  /// The local file for the background image of the avatar.
  final File? file;

  /// A widget to display in the center of the avatar.
  ///
  /// This widget is displayed on top of the background image or color of the
  /// avatar.
  final Widget? child;

  /// A callback function to be called when the avatar is tapped.
  final VoidCallback? onTap;

  /// Whether the avatar should have a border around it.
  ///
  /// If true, a border will be added around the avatar. Defaults to true.
  final bool hasBorder;

  /// Flag for when the image is being loaded from the url
  final bool loading;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);

    Widget? placeholder;
    ImageProvider<Object>? backgroundImage;
    ImageProvider<Object>? foregroundImage;

    if (!loading) {
      if (file != null) {
        backgroundImage = FileImage(file!);
      } else if (url != null) {
        foregroundImage = CachedNetworkImageProvider(
          url!,
          maxHeight: _kDecodeSize,
          maxWidth: _kDecodeSize,
        );
      } else {
        placeholder = child ?? Center(child: MPlaceholder(dimension: radius));
      }
    }

    final Widget avatar = CircleAvatar(
      radius: radius,
      foregroundImage: foregroundImage,
      backgroundImage: backgroundImage,
      backgroundColor: colors.surfaceShade,
      child: placeholder,
    );

    return Skeleton.leaf(
      child: GestureDetector(
        onTap: onTap,
        child: CircleAvatar(
          radius: radius,
          backgroundColor: colors.outlineVariant3,
          child: Padding(
            padding: hasBorder
                ? const EdgeInsets.all(Strokes.thin)
                : EdgeInsets.zero,
            child: avatar,
          ),
        ),
      ),
    );
  }
}
