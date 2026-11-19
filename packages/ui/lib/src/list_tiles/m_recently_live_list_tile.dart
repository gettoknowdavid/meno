import 'package:cached_network_image/cached_network_image.dart';
import 'package:intl/intl.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:skeletonizer/skeletonizer.dart';

/// A widget that displays information about a recently live event.
///
/// This widget provides a tile layout for displaying details about a recently
/// live event, including the event title, end time, creator, and an optional
/// image. It also supports handling tap events and showing a loading state.
///
/// To use this widget, provide the necessary parameters such as the title,
/// end time, creator, and optional image URL. You can also specify whether the
/// tile is in a loading state
/// and provide a callback function for handling tap events.
///
/// Example usage:
/// ```dart
/// MRecentlyLiveListTile(
///   title: 'Live Concert',
///   endTime: DateTime.now(),
///   creator: 'John Doe',
///   imageUrl: 'https://example.com/image.jpg',
///   onTap: () {
///     // Handle tap event
///   },
/// );
/// ```
class MRecentlyLiveListTile extends StatelessWidget {
  /// Creates an instance of [MRecentlyLiveListTile].
  ///
  /// Parameters:
  /// - [key]: An optional key to identify the widget.
  /// - [title]: The title of the recently live event.
  /// - [endTime]: An optional end time of the recently live event.
  /// - [creator]: An optional creator of the recently live event.
  /// - [imageUrl]: An optional URL for the event image.
  /// - [onTap]: A callback function to handle tap events.
  const new({
    super.key,
    this.title,
    this.endTime,
    this.startTime,
    this.creator,
    this.imageUrl,
    this.onTap,
  });

  /// The title of the recently live event.
  final String? title;

  /// An optional end time of the recently live event.
  final DateTime? endTime;

  /// When the event is scheduled to start, for a row that has not run yet.
  ///
  /// A scheduled broadcast has no end time, so the relative "ago" label had
  /// nothing to measure and fell back to "A while back" — a statement about a
  /// broadcast that had not happened. This is the other fact the same line can
  /// state, and it is shown in the reader's own zone.
  final DateTime? startTime;

  /// An optional creator of the recently live event.
  final String? creator;

  /// An optional URL for the event image.
  final String? imageUrl;

  /// A callback function to handle tap events.
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return ListTile(
      onTap: onTap,
      contentPadding: const .symmetric(horizontal: 12, vertical: 14),
      shape: RoundedRectangleBorder(
        side: BorderSide(color: colors.outlineVariant1),
        borderRadius: Corners.lg,
      ),
      minLeadingWidth: 12,
      leading: _Artwork(imageUrl: imageUrl),
      title: Column(
        mainAxisAlignment: .spaceAround,
        crossAxisAlignment: .start,
        children: [
          MText(
            _label(),
            style: textTheme.captionRegular,
            color: colors.onBackgroundVariant,
            maxLines: 1,
            overflow: .ellipsis,
          ),
          Spaces.verticalMicro,
          MText(
            title!,
            style: textTheme.captionMedium,
            maxLines: 1,
            overflow: .ellipsis,
          ),
          Spaces.verticalMicro,
          MText(
            creator!,
            style: textTheme.captionRegular,
            color: colors.onBackgroundVariant,
            maxLines: 1,
            overflow: .ellipsis,
          ),
        ],
      ),
    );
  }

  /// Formats a [DateTime] as a relative time string.
  ///
  /// The one line above the title says when this row's moment is: an end time
  /// once it has run, a start time while it is still ahead, and the honest
  /// fallback when the API stated neither.
  String _label() {
    final ends = endTime;
    if (ends != null) return _timeAgo(ends);

    final starts = startTime;
    if (starts != null) {
      return 'Scheduled for ${DateFormat('EEE, MMM d · h:mm a').format(starts.toLocal())}';
    }

    return 'A while back';
  }

  String _timeAgo(DateTime dateTime) {
    final now = DateTime.now();
    final difference = now.difference(dateTime);

    if (difference.inSeconds < 60) {
      return 'Just now';
    } else if (difference.inMinutes < 60) {
      return '${difference.inMinutes}m ago';
    } else if (difference.inHours < 24) {
      return '${difference.inHours}h ago';
    } else if (difference.inDays < 7) {
      return '${difference.inDays}d ago';
    } else {
      return '${difference.inDays ~/ 7}w ago';
    }
  }
}

class _Artwork extends StatelessWidget {
  const new({this.imageUrl});

  final String? imageUrl;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);

    final colorFilter = ColorFilter.mode(colors.onSurfaceShade, .srcIn);

    Widget? imageWidget;

    if (imageUrl == null) {
      imageWidget = Center(
        child: Assets.images.logoLight.svg(
          package: 'meno_ui',
          colorFilter: colorFilter,
          height: 32,
        ),
      );
    } else {
      imageWidget = CachedNetworkImage(
        imageUrl: imageUrl!,
        placeholder: (context, url) => Container(
          width: 79,
          height: 68,
          decoration: const BoxDecoration(borderRadius: Corners.md),
        ),
        imageBuilder: (context, image) => DecoratedBox(
          decoration: BoxDecoration(
            image: DecorationImage(image: image, fit: .cover),
            borderRadius: Corners.md,
          ),
        ),
      );
    }

    return Skeleton.leaf(
      child: Container(
        width: 79,
        height: 68,
        color: colors.background,
        child: imageWidget,
      ),
    );
  }
}
