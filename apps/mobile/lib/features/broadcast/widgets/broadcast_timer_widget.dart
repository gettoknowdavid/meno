import 'package:flutter/material.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:meno_ui/src/cards/m_timer_display.dart';

class BroadcastTimerWidget extends StatelessWidget {
  const new({
    required this.formattedTime,
    this.isRunning = false,
    this.timeAgo = '',
    super.key,
    this.showTimeAgo = true,
    this.textStyle,
    this.color,
  });

  final String formattedTime;
  final bool isRunning;
  final String timeAgo;
  final bool showTimeAgo;
  final TextStyle? textStyle;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    return MTimerDisplay(
      formattedTime: formattedTime,
      isRunning: isRunning,
      timeAgo: timeAgo,
      showTimeAgo: showTimeAgo,
      textStyle: textStyle,
      color: color,
    );
  }
}
