import 'package:material_ui/material_ui.dart';

/// The app bar the notes screens pin, lifted out of the pages that used to
/// spell it out field by field.
///
/// It is a [SliverAppBar] with the behaviour those screens rely on baked in:
/// pinned, without a stretch, so the search field in [bottom] stays where it is
/// while the body scrolls under it. Pages wrap it in a [SliverOverlapAbsorber]
/// when they embed their own scrollable, which is why `forceElevated` and
/// `shadowColor` are forwarded — the shadow is the overlap's visual cue.
class DynamicSliverAppBar extends StatelessWidget {
  /// Creates the app bar.
  const new({
    super.key,
    this.title,
    this.actions,
    this.bottom,
    this.flexibleSpace,
    this.expandedHeight,
    this.toolbarHeight = kToolbarHeight,
    this.backgroundColor,
    this.surfaceTintColor,
    this.shadowColor,
    this.elevation = 0,
    this.forceElevated = false,
    this.pinned = true,
  });

  /// What the bar shows next to the leading edge.
  final Widget? title;

  /// The actions on the trailing edge.
  final List<Widget>? actions;

  /// A strip under the toolbar, such as a search field.
  final PreferredSizeWidget? bottom;

  /// A widget behind the toolbar and bottom.
  final Widget? flexibleSpace;

  /// How tall the app bar is when it is fully expanded.
  final double? expandedHeight;

  /// How tall the toolbar itself is.
  final double toolbarHeight;

  /// The bar's colour.
  final Color? backgroundColor;

  /// The tint a scroll-under surface would take.
  final Color? surfaceTintColor;

  /// The colour of the shadow cast once content scrolls under it.
  final Color? shadowColor;

  /// The bar's elevation.
  final double elevation;

  /// Whether to keep the shadow even when the body has not scrolled.
  final bool forceElevated;

  /// Whether the bar stays at the top of the screen.
  final bool pinned;

  @override
  Widget build(BuildContext context) {
    return SliverAppBar(
      title: title,
      actions: actions,
      bottom: bottom,
      flexibleSpace: flexibleSpace,
      expandedHeight: expandedHeight,
      toolbarHeight: toolbarHeight,
      backgroundColor: backgroundColor,
      surfaceTintColor: surfaceTintColor,
      shadowColor: shadowColor,
      elevation: elevation,
      forceElevated: forceElevated,
      pinned: pinned,
    );
  }
}
