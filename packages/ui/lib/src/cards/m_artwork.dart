import 'package:cached_network_image/cached_network_image.dart';
import 'package:flutter/material.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// A broadcast artwork/cover image display widget.
///
/// Displays a rounded image from a URL with a placeholder while loading.
/// When no URL is provided, shows an app logo placeholder.
///
/// Example usage:
/// ```dart
/// MBroadcastArtwork(
///   imageUrl: 'https://example.com/cover.jpg',
///   size: const Size(201, 128),
/// )
/// ```
class MBroadcastArtwork extends StatelessWidget {
  /// Creates an [MBroadcastArtwork] widget.
  const new({
    super.key,
    this.imageUrl,
    this.size = const Size(201, 128),
    this.borderRadius = Corners.lg,
    this.placeholderColorFilter,
  });

  /// The URL of the artwork image to display.
  final String? imageUrl;

  /// The size of the artwork container.
  final Size size;

  /// The border radius for the artwork container.
  final BorderRadius borderRadius;

  /// Optional color filter for the placeholder when no image is available.
  final ColorFilter? placeholderColorFilter;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);

    Widget imageWidget;

    if (imageUrl != null) {
      imageWidget = CachedNetworkImage(
        imageUrl: imageUrl!,
        width: size.width,
        height: size.height,
        imageBuilder: (context, imageProvider) => DecoratedBox(
          decoration: BoxDecoration(
            borderRadius: borderRadius,
            image: DecorationImage(image: imageProvider, fit: BoxFit.cover),
          ),
        ),
        placeholder: (context, url) => const MShimmer(borderRadius: 16),
      );
    } else {
      final colorFilter =
          placeholderColorFilter ??
          ColorFilter.mode(colors.onSurfaceShade, BlendMode.srcIn);
      imageWidget = Center(
        child: Assets.images.logoLight.svg(
          colorFilter: colorFilter,
          height: 32,
        ),
      );
    }

    return Container(
      width: size.width,
      height: size.height,
      decoration: BoxDecoration(
        color: colors.surfaceShade,
        borderRadius: borderRadius,
      ),
      child: imageWidget,
    );
  }
}
