import 'package:intl/intl.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// A widget that represents a card for live content.
///
/// This card displays information about live content, including the title,
/// host, and an optional image. It also supports showing a live count.
/// The card can handle tap gestures through the [onTap] callback.
///
/// Example usage:
/// ```dart
/// MLiveCard(
///   title: 'Live Event Title',
///   host: 'Host Name',
///   loading: false,
///   imageUrl: 'https://example.com/image.jpg',
///   liveCount: 1234,
///   onTap: () {
///     // Handle card tap
///   },
/// )
/// ```
class MLiveCard extends StatelessWidget {
  /// Creates an instance of [MLiveCard].
  ///
  /// Parameters:
  /// - [title]: The title of the live content.
  /// - [host]: The name of the host of the live content.
  /// - [key]: An optional key to identify the widget.
  /// - [imageUrl]: An optional URL for an image to be displayed in the card.
  /// - [liveCount]: An optional count of live viewers or participants.
  /// - [onTap]: An optional callback function to be invoked when the card is
  ///   tapped.
  const new({
    this.title,
    this.host,
    super.key,
    this.imageUrl,
    this.liveCount = 0,
    this.onTap,
  });

  /// The title of the live content.
  final String? title;

  /// The name of the host of the live content.
  final String? host;

  /// The URL of an image to be displayed in the card.
  final String? imageUrl;

  /// The count of live viewers or participants.
  final int? liveCount;

  /// A callback function to be invoked when the card is tapped.
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final styles = MCardStyles.of(context);

    String? count;

    if (liveCount != null && liveCount != 0) {
      count = NumberFormat.compactCurrency(
        decimalDigits: 0,
        symbol: '',
      ).format(liveCount);
    }

    return GestureDetector(
      onTap: onTap,
      child: Stack(
        clipBehavior: Clip.none,
        children: [
          _Container(
            children: [
              MAvatar(radius: 44, url: imageUrl),
              const SizedBox(height: 12),
              SizedBox(
                height: 24,
                child: MText(
                  title!,
                  style: styles.titleStyle,
                  color: styles.titleColor,
                  maxLines: 1,
                  textAlign: TextAlign.center,
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              const SizedBox(height: 4),
              MText(
                host!,
                style: styles.hostStyle,
                color: styles.hostColor,
                maxLines: 1,
                textAlign: TextAlign.center,
                overflow: TextOverflow.ellipsis,
              ),
            ],
          ),
          Positioned(
            top: 8,
            left: 16,
            child: MBadge.live(count: count, showBorder: true),
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
      child: SizedBox(
        width: 144,
        height: 144,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          mainAxisAlignment: MainAxisAlignment.center,
          children: children,
        ),
      ),
    );
  }
}
