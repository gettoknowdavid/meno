import 'package:flutter/material.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// A timer display widget for broadcasting sessions.
///
/// Shows the formatted time (HH:MM:SS) and optionally a "time ago" string.
///
/// Example usage:
/// ```dart
/// MTimerDisplay(
///   formattedTime: '00:15:30',
///   isRunning: true,
///   timeAgo: '15 minutes ago',
/// )
/// ```
class MTimerDisplay extends StatelessWidget {
  /// Creates an [MTimerDisplay] widget.
  const new({
    required this.formattedTime,
    this.isRunning = false,
    this.timeAgo = '',
    super.key,
    this.showTimeAgo = true,
    this.textStyle,
    this.color,
  });

  /// The formatted time string (HH:MM:SS).
  final String formattedTime;

  /// Whether the timer is currently running.
  final bool isRunning;

  /// The "time ago" string to display.
  final String timeAgo;

  /// Whether to show the time ago string.
  final bool showTimeAgo;

  /// Optional text style override.
  final TextStyle? textStyle;

  /// Optional color override.
  final Color? color;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    final displayColor = color ?? colors.onDisabledContainer;
    final displayStyle = textStyle ?? textTheme.captionRegular;

    return Row(
      mainAxisAlignment: .center,
      children: [
        // Formatted time (HH:MM:SS)
        MText(formattedTime, style: displayStyle.copyWith(color: displayColor)),

        // Time ago display (optional)
        if (isRunning && showTimeAgo) ...[
          Spaces.horizontalSmall,
          const MDot(),
          Spaces.horizontalSmall,
          MText(timeAgo, style: displayStyle.copyWith(color: displayColor)),
        ],
      ],
    );
  }
}
