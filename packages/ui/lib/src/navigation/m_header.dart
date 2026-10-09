import 'package:flutter/material.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// A widget representing a header with a title, optional action, and styling
/// options.
class MHeader extends StatelessWidget {
  /// Creates a new `MHeader` widget.
  ///
  /// - [title]: The text displayed as the header title.
  /// - [action]: An optional widget to display on the right side of the header.
  /// - [showSideBorder]: Whether to display a side border (default: true).
  /// - [padding]: The padding around the header content (optional).
  /// - [addTopMargin]: Whether to add a top margin to the header
  /// (default: false).
  const new({
    required this.title,
    super.key,
    this.action,
    this.showSideBorder = true,
    this.padding,
    this.addTopMargin = false,
  });

  /// The title of the header.
  final String title;

  /// An optional widget to display on the right side of the header.
  final Widget? action;

  /// Whether to display a side border.
  final bool showSideBorder;

  /// The padding around the header content.
  final EdgeInsetsGeometry? padding;

  /// Whether to add a top margin to the header.
  final bool addTopMargin;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);
    return Container(
      color: colors.background,
      height: 30,
      margin: addTopMargin ? const .only(top: 8) : null,
      padding: padding ?? const .fromLTRB(16, 0, 16, 0),
      child: Row(
        children: [
          if (showSideBorder) ...[
            Container(
              width: 3,
              margin: const .symmetric(vertical: 2),
              color: colors.error,
            ),
            Spaces.horizontalMicro,
          ],
          MText(
            title,
            style: textTheme.heading3Bold,
            color: colors.onBackground,
          ),
          const Spacer(),
          if (action != null) ...[action!],
        ],
      ),
    );
  }
}
