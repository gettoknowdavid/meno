import 'package:flutter/material.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// A stack of participant avatars with a count badge for overflow.
///
/// Displays up to 3 avatars in an overlapping stack with a "+N" badge
/// when there are more participants. Used in ended broadcast summaries.
///
/// Example usage:
/// ```dart
/// MAvatarStack(
///   participants: participants,
///   totalCount: totalCount,
///   avatarSize: 40,
/// )
/// ```
class MAvatarStack extends StatelessWidget {
  /// Creates an [MAvatarStack] widget.
  const new({
    required this.participants,
    required this.totalCount,
    super.key,
    this.avatarSize = 40,
  });

  /// The list of participants to display.
  final List<ParticipantAvatar> participants;

  /// The total number of participants (for the overflow badge).
  final int totalCount;

  /// The size of each avatar in the stack.
  final double avatarSize;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    // Show max 3 avatars
    final displayCount = participants.length.clamp(0, 3);
    final hasMore = totalCount > 3;

    // Calculate stack width based on number of avatars
    final stackWidth = _calculateStackWidth(displayCount, hasMore);

    if (displayCount == 0 && !hasMore) {
      return const SizedBox.shrink();
    }

    return SizedBox(
      height: avatarSize,
      width: stackWidth,
      child: Stack(
        alignment: Alignment.center,
        children: [
          // Show avatars (max 3)
          ...List.generate(
            displayCount,
            (index) => Positioned(
              left: index * 24.0,
              child: _StackedAvatar(
                imageUrl: participants[index].avatarUrl,
                size: avatarSize,
                colors: colors,
              ),
            ),
          ),

          // Show count badge if more than 3
          if (hasMore)
            Positioned(
              left: displayCount * 24.0,
              child: _CountBadge(
                count: totalCount - 3,
                size: avatarSize,
                backgroundColor: colors.primary,
                textStyle: textTheme.captionMedium,
                colors: colors,
              ),
            ),
        ],
      ),
    );
  }

  double _calculateStackWidth(int displayCount, bool hasMore) {
    if (displayCount == 0) return 0;

    // Base: first avatar (avatarSize) + overlapping avatars (24px each)
    const overlapWidth = 24.0;

    // Width = first avatar + (remaining avatars * overlap) + (badge if needed)
    final baseWidth = avatarSize + ((displayCount - 1) * overlapWidth);
    final badgeWidth = hasMore ? overlapWidth + avatarSize : 0;

    return baseWidth + badgeWidth;
  }
}

class _StackedAvatar extends StatelessWidget {
  const new({required this.imageUrl, required this.size, required this.colors});

  final String? imageUrl;
  final double size;
  final MColorScheme colors;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: size,
      height: size,
      decoration: BoxDecoration(
        shape: BoxShape.circle,
        border: Border.all(color: colors.background, width: 2),
      ),
      child: MAvatar(radius: size / 2, url: imageUrl, hasBorder: false),
    );
  }
}

class _CountBadge extends StatelessWidget {
  const new({
    required this.count,
    required this.size,
    required this.backgroundColor,
    required this.textStyle,
    required this.colors,
  });

  final int count;
  final double size;
  final Color backgroundColor;
  final TextStyle textStyle;
  final MColorScheme colors;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: size,
      height: size,
      decoration: BoxDecoration(
        shape: BoxShape.circle,
        color: backgroundColor,
        border: Border.all(color: colors.background, width: 2),
      ),
      child: Center(
        child: MText(
          '+$count',
          style: textStyle.copyWith(color: colors.onPrimary),
        ),
      ),
    );
  }
}

/// A lightweight participant data class for the avatar stack.
class ParticipantAvatar {
  /// Creates a new [ParticipantAvatar] with the given [id] and [avatarUrl].
  const new({required this.id, this.avatarUrl});

  /// The unique identifier of the participant.
  final String id;

  /// The URL of the participant's avatar.
  final String? avatarUrl;
}
