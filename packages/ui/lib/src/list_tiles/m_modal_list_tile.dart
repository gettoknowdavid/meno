import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:skeletonizer/skeletonizer.dart';

/// A customizable list tile widget for use in modals.
///
/// This widget allows you to create a list tile that can be used within a
/// modal dialog. It supports a title, leading and trailing icons, and an
/// optional divider. It also includes options for handling tap events and
/// displaying a loading indicator.
///
/// To use this widget, provide the necessary parameters such as the title and
/// optional leading and trailing widgets. You can also specify whether to show
/// a divider, handle tap events, and configure the title color.
///
/// Example usage:
/// ```dart
/// MModalListTile(
///   title: 'Settings',
///   leading: Icon(Icons.settings),
///   trailing: Icons.chevron_right,
///   onTap: () {
///     // Handle tap event
///   },
/// );
/// ```
class MModalListTile extends StatelessWidget {
  /// Creates an instance of [MModalListTile].
  ///
  /// Parameters:
  /// - [key]: An optional key to identify the widget.
  /// - [title]: The title of the list tile.
  /// - [leading]: An optional widget to display before the title.
  /// - [trailing]: An optional icon to display after the title.
  /// - [showDivider]: A boolean indicating whether to show a divider below
  /// the tile. Defaults to false.
  /// - [onTap]: A callback function to handle tap events.
  /// - [titleColor]: The color of the title text.
  /// - [loading]: A boolean indicating whether the loading state is active.
  /// Defaults to false.
  const new({
    super.key,
    this.title,
    this.leading,
    this.trailing,
    this.showDivider = false,
    this.onTap,
    this.titleColor,
    this.loading = false,
    this.contentPadding,
  });

  /// The title of the list tile.
  final String? title;

  /// An optional widget to display before the title.
  final Widget? leading;

  /// An optional icon to display after the title.
  final IconData? trailing;

  /// A boolean indicating whether to show a divider below the tile. Defaults
  /// to false.
  final bool showDivider;

  /// A callback function to handle tap events.
  final VoidCallback? onTap;

  /// The color of the title text.
  final Color? titleColor;

  /// A boolean indicating whether the loading state is active. Defaults to
  /// false.
  final bool loading;

  /// Content padding for the list tile
  final EdgeInsetsGeometry? contentPadding;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    final iconTheme = IconThemeData(
      size: 20,
      color: titleColor ?? colors.onBackground,
    );

    return SizedBox(
      height: showDivider ? 56.0 : null,
      child: Column(
        children: [
          ListTile(
            onTap: onTap,
            minTileHeight: 40,
            title: title != null
                ? Skeletonizer(
                    enabled: loading,
                    child: MText(
                      title!,
                      style: textTheme.bodyRegular,
                      color: titleColor,
                    ),
                  )
                : null,
            minLeadingWidth: Insets.md,
            leading: IconTheme(
              data: iconTheme,
              child: SizedBox.square(
                dimension: 24,
                child: Skeletonizer(
                  enabled: loading,
                  child: leading ?? const SizedBox(),
                ),
              ),
            ),
            trailing: SizedBox.square(
              dimension: 24,
              child: trailing != null
                  ? IconTheme(data: iconTheme, child: Icon(trailing))
                  : null,
            ),
            contentPadding: contentPadding ?? const .fromLTRB(12, 0, 12, 0),
          ),
          if (showDivider)
            const MDivider(bottomSpace: Insets.sm, topSpace: Insets.sm),
        ],
      ),
    );
  }
}
