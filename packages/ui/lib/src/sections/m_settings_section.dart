import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/text/m_text.dart';
import 'package:meno_ui/src/theme/extensions/m_color_scheme.dart';
import 'package:meno_ui/src/theme/extensions/m_text_theme.dart';
import 'package:meno_ui/src/tokens/corners.dart';
import 'package:meno_ui/src/tokens/spacing.dart';

/// A titled group of settings rows.
///
/// Draws an optional muted title above a rounded, clipped container holding
/// [children] — typically `MSettingsListTile`s. The container clips so its
/// children share its corner radius.
///
/// Example usage:
/// ```dart
/// MSettingsSection(
///   title: 'General',
///   children: [MSettingsListTile(title: 'Language')],
/// )
/// ```
class MSettingsSection extends StatelessWidget {
  /// Creates a settings section.
  ///
  /// - [children]: The rows shown inside the group.
  /// - [title]: An optional group title above the container.
  /// - [titleContainerHeight]: An optional fixed height for the title area.
  const new({
    required this.children,
    super.key,
    this.title,
    this.titleContainerHeight,
  });

  /// The section title.
  final String? title;

  /// The rows shown inside the group.
  final List<Widget> children;

  /// An optional fixed height for the title area.
  final double? titleContainerHeight;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (title != null)
          SizedBox(
            height: titleContainerHeight,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                MText(
                  title!,
                  style: textTheme.captionMedium,
                  color: colors.inActive,
                ),
                Spaces.verticalSmall,
              ],
            ),
          ),
        Container(
          clipBehavior: Clip.hardEdge,
          decoration: ShapeDecoration(
            shape: const RoundedRectangleBorder(borderRadius: Corners.lg),
            color: colors.surfaceTint,
          ),
          child: Material(child: Column(children: children)),
        ),
      ],
    );
  }
}
