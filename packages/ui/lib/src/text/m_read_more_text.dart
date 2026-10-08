import 'package:flutter/material.dart';
import 'package:meno_ui/src/styles.dart';
import 'package:meno_ui/src/text/m_text.dart';
import 'package:meno_ui/src/theme/m_color_scheme.dart';
import 'package:meno_ui/src/theme/styles/m_text_theme.dart';

/// A widget that allows users to expand and collapse long text.
///
/// This widget displays a portion of the provided text with an option to
/// reveal the full content.
class MReadMoreText extends StatefulWidget {
  /// Creates a new `MReadMoreText` widget.
  ///
  /// * `text`: The full text content to be displayed.
  /// * `style`: An optional style to apply to the text.
  /// * `maxLines`: The initial number of lines to display before truncation
  /// (default: 3).
  const new(this.text, {super.key, this.style, this.maxLines = 3});

  /// The full text content to be displayed.
  final String text;

  /// An optional style to apply to the text.
  final TextStyle? style;

  /// The initial number of lines to display before truncation.
  final int maxLines;

  @override
  State<MReadMoreText> createState() => _MReadMoreTextState();
}

class _MReadMoreTextState extends State<MReadMoreText> {
  late int effectiveMaxLines;
  late bool isReadMore;

  @override
  void initState() {
    super.initState();
    effectiveMaxLines = widget.maxLines;
    isReadMore = false;
  }

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        MText(
          widget.text,
          style: widget.style ?? textTheme.captionRegular,
          maxLines: isReadMore ? null : effectiveMaxLines,
          overflow: isReadMore ? null : TextOverflow.ellipsis,
        ),
        Spaces.verticalMicro,
        GestureDetector(
          onTap: () => isReadMore = !isReadMore,
          child: MText(
            isReadMore ? 'less' : 'more',
            style: textTheme.captionMedium,
            color: colors.onBackgroundVariant,
          ),
        ),
      ],
    );
  }
}
