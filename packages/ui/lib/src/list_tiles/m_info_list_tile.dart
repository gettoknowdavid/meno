import 'package:flutter/material.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/text/m_text.dart';
import 'package:meno_ui/src/theme/extensions/m_color_scheme.dart';
import 'package:meno_ui/src/theme/extensions/m_text_theme.dart';
import 'package:meno_ui/src/tokens/corners.dart';
import 'package:meno_ui/src/tokens/spacing.dart';

/// A list tile with leading text and subtitle for form/editor displays.
///
/// Displays a labeled row with a leading text label and a subtitle description,
/// commonly used in broadcast editor and settings forms.
///
/// Example usage:
/// ```dart
/// MInfoListTile(
///   leadingText: 'Remaining time today',
///   subtitleText: 'Your daily broadcast time will reset in 24hrs',
///   trailing: MText('0hr 30min'),
/// )
/// ```
class MInfoListTile extends StatelessWidget {
  /// Creates an [MInfoListTile] widget.
  const new({
    required this.leadingText,
    required this.subtitleText,
    super.key,
    this.trailing,
  });

  /// The leading label text.
  final String leadingText;

  /// The subtitle/description text below the label.
  final String subtitleText;

  /// Optional trailing widget (e.g., a value display or switch).
  final Widget? trailing;

  @override
  Widget build(BuildContext context) {
    final theme = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return Column(
      crossAxisAlignment: .start,
      children: [
        Container(
          padding: const .fromLTRB(Insets.lg, Insets.sm, Insets.lg, Insets.sm),
          decoration: BoxDecoration(
            color: theme.disabled,
            borderRadius: Corners.sm,
          ),
          child: SizedBox(
            height: 24,
            child: Row(
              mainAxisAlignment: .spaceBetween,
              children: [
                Expanded(
                  child: MText(leadingText, style: textTheme.captionMedium),
                ),
                Spaces.horizontalLarge,
                trailing ?? const SizedBox(),
              ],
            ),
          ),
        ),
        const SizedBox(height: 6),
        SizedBox(
          height: 18,
          child: MText(subtitleText, style: textTheme.captionRegular),
        ),
      ],
    );
  }
}
