import 'package:cached_network_image/cached_network_image.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:skeletonizer/skeletonizer.dart';

/// A widget that represents a card for recently live content.
///
/// This card displays information about recently live content, including the
/// title of the content, the host, and an optional image. The card can also
/// handle tap gestures through the [onTap] callback.
///
/// Example usage:
/// ```dart
/// MRecentlyLiveCard(
///   title: 'Live Event Title',
///   host: 'Host Name',
///   imageUrl: 'https://example.com/image.jpg',
///   onTap: () {
///     // Handle card tap
///   },
/// )
/// ```
class MRecentlyLiveCard extends StatelessWidget {
  /// Creates an instance of [MRecentlyLiveCard].
  ///
  /// Parameters:
  /// - [title]: The title of the recently live content.
  /// - [host]: The name of the host of the recently live content.
  /// - [key]: An optional key to identify the widget.
  /// - [imageUrl]: An optional URL for an image to be displayed in the card.
  /// - [onTap]: An optional callback function to be invoked when the card is
  ///   tapped.
  const new({this.title, this.host, super.key, this.imageUrl, this.onTap});

  /// The title of the recently live content.
  final String? title;

  /// The name of the host of the recently live content.
  final String? host;

  /// The URL of an image to be displayed in the card.
  final String? imageUrl;

  /// A callback function to be invoked when the card is tapped.
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final styles = MCardStyles.of(context);
    final colors = MColorScheme.of(context);
    final isLight = colors.brightness == Brightness.light;

    final colorFilter = ColorFilter.mode(
      isLight ? MColor.grey200 : MColor.grey30,
      BlendMode.srcIn,
    );

    final hasImage = imageUrl != null;

    Widget? placeholder;
    DecorationImage? image;
    if (hasImage) {
      image = DecorationImage(
        image: CachedNetworkImageProvider(imageUrl!),
        fit: BoxFit.cover,
      );
    } else {
      placeholder = Assets.images.logoLight.svg(
        package: 'meno_ui',
        colorFilter: colorFilter,
        height: 32,
      );
    }

    return GestureDetector(
      onTap: onTap,
      child: _Container(
        children: [
          Skeleton.leaf(
            child: Container(
              width: 148,
              height: 88,
              padding: const EdgeInsets.symmetric(vertical: 28),
              clipBehavior: Clip.antiAlias,
              decoration: BoxDecoration(
                color: isLight ? MColor.grey30 : MColor.grey400,
                borderRadius: Corners.md,
                image: image,
              ),
              child: !hasImage ? placeholder : null,
            ),
          ),
          const SizedBox(height: 12),
          MText(
            title!,
            style: styles.titleStyle,
            color: styles.titleColor,
            textAlign: TextAlign.center,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
          ),
          const SizedBox(height: 4),
          MText(
            host ?? '',
            style: styles.hostStyle,
            color: styles.hostColor,
            textAlign: TextAlign.center,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
          ),
        ],
      ),
    );
  }
}

class _Container extends StatelessWidget {
  const new({required this.children});
  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    final styles = MCardStyles.of(context);
    return Container(
      width: 176,
      height: 176,
      padding: const EdgeInsets.all(14),
      decoration: ShapeDecoration(
        color: styles.backgroundColor,
        shadows: Shadows.soft,
        shape: Corners.squircleLg,
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        mainAxisAlignment: MainAxisAlignment.center,
        children: children,
      ),
    );
  }
}
