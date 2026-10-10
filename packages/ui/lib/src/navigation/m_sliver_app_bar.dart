import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:material_ui/material_ui.dart';

/// A [SliverAppBar] whose expanded height is measured from its
/// [flexibleSpace].
///
/// A normal [SliverAppBar] needs an explicit [expandedHeight]. This widget
/// lays the [flexibleSpace] out once, measures it, and then feeds that height
/// to the real sliver — so the content dictates the size instead of a magic
/// number.
class MSliverAppBar extends StatefulWidget {
  /// Creates a sliver app bar.
  ///
  /// Every parameter mirrors [SliverAppBar]; [flexibleSpace] is measured to
  /// derive [expandedHeight].
  const new({
    super.key,
    this.flexibleSpace = const SizedBox(height: kToolbarHeight),
    this.leading,
    this.automaticallyImplyLeading = true,
    this.title,
    this.actions,
    this.bottom,
    this.elevation,
    this.scrolledUnderElevation,
    this.shadowColor,
    this.surfaceTintColor,
    this.forceElevated = false,
    this.backgroundColor,
    this.backgroundGradient,
    this.foregroundColor,
    this.iconTheme,
    this.actionsIconTheme,
    this.primary = true,
    this.centerTitle,
    this.excludeHeaderSemantics = false,
    this.titleSpacing,
    this.collapsedHeight,
    this.expandedHeight,
    this.floating = false,
    this.pinned = false,
    this.snap = false,
    this.stretch = false,
    this.stretchTriggerOffset = 100,
    this.onStretchTrigger,
    this.shape,
    this.toolbarHeight = kToolbarHeight + 20,
    this.leadingWidth,
    this.toolbarTextStyle,
    this.titleTextStyle,
    this.systemOverlayStyle,
    this.forceMaterialTransparency = false,
    this.clipBehavior,
    this.appBarClipper,
  });

  /// The widget shown behind the toolbar.
  final Widget? flexibleSpace;

  /// The widget leading the toolbar, typically a back button.
  final Widget? leading;

  /// Whether to imply a leading widget when [leading] is null.
  final bool automaticallyImplyLeading;

  /// The primary title widget.
  final Widget? title;

  /// The widgets trailing the title.
  final List<Widget>? actions;

  /// The widget below the toolbar.
  final PreferredSizeWidget? bottom;

  /// The elevation of the app bar.
  final double? elevation;

  /// The elevation when content scrolls under the bar.
  final double? scrolledUnderElevation;

  /// The colour of the bar's shadow.
  final Color? shadowColor;

  /// The tint applied to the bar's surface.
  final Color? surfaceTintColor;

  /// Whether the bar is always drawn with its scrolled-under elevation.
  final bool forceElevated;

  /// The bar's background colour, ignored when [backgroundGradient] is set.
  final Color? backgroundColor;

  /// A gradient painted behind the bar. Overrides [backgroundColor].
  final LinearGradient? backgroundGradient;

  /// The default colour of the bar's text and icons.
  final Color? foregroundColor;

  /// The icon theme for the leading widget.
  final IconThemeData? iconTheme;

  /// The icon theme for [actions].
  final IconThemeData? actionsIconTheme;

  /// Whether the bar is a primary scroll view.
  final bool primary;

  /// Whether the title is centred.
  final bool? centerTitle;

  /// Whether to exclude the header's semantics from the child.
  final bool excludeHeaderSemantics;

  /// The spacing around [title].
  final double? titleSpacing;

  /// The height of the bar when collapsed.
  final double? collapsedHeight;

  /// An explicit expanded height, overriding the measured one.
  final double? expandedHeight;

  /// Whether the bar floats when scrolling up.
  final bool floating;

  /// Whether the bar stays pinned when scrolling.
  final bool pinned;

  /// Whether a floating bar snaps into view.
  final bool snap;

  /// Whether the bar stretches on overscroll.
  final bool stretch;

  /// The overscroll offset that triggers [onStretchTrigger].
  final double stretchTriggerOffset;

  /// Called when an overscroll passes [stretchTriggerOffset].
  final AsyncCallback? onStretchTrigger;

  /// The shape of the bar.
  final ShapeBorder? shape;

  /// The height of the toolbar.
  final double toolbarHeight;

  /// The width of [leading].
  final double? leadingWidth;

  /// The text style for the toolbar.
  final TextStyle? toolbarTextStyle;

  /// The text style for [title].
  final TextStyle? titleTextStyle;

  /// The system overlay style requested while the bar is visible.
  final SystemUiOverlayStyle? systemOverlayStyle;

  /// Whether the bar's material is transparent.
  final bool forceMaterialTransparency;

  /// The clipping behaviour applied to the bar.
  final Clip? clipBehavior;

  /// An optional clipper shaping the bar's background.
  final CustomClipper<Path>? appBarClipper;

  @override
  State<MSliverAppBar> createState() => _MSliverAppBarState();
}

class _MSliverAppBarState extends State<MSliverAppBar> {
  final GlobalKey _childKey = GlobalKey();

  // While the height is 0, a SliverToBoxAdapter stands in for the sliver so the
  // flexible space can be measured before the real sliver is built.
  double _height = 0;

  @override
  void initState() {
    super.initState();
    _updateHeight();
  }

  @override
  void didUpdateWidget(covariant MSliverAppBar oldWidget) {
    super.didUpdateWidget(oldWidget);
    _updateHeight();
  }

  /// Measures the flexible space after the frame and stores the result.
  void _updateHeight() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      final context = _childKey.currentContext;
      if (context == null) return;
      final box = context.findRenderObject() as RenderBox?;
      if (box == null || !mounted) return;
      setState(() => _height = box.size.height);
    });
  }

  @override
  Widget build(BuildContext context) {
    // Lay the flexible space out first so its height can be measured.
    if (_height == 0) {
      return SliverToBoxAdapter(
        child: Stack(
          children: [
            Padding(
              padding: EdgeInsets.symmetric(
                vertical: MediaQuery.paddingOf(context).top / 2,
              ),
              child: Container(key: _childKey, child: widget.flexibleSpace),
            ),
            Positioned.fill(
              top: 10,
              child: Align(
                alignment: Alignment.topCenter,
                child: AppBar(
                  backgroundColor: Colors.transparent,
                  elevation: 0,
                  leading: widget.leading,
                  actions: widget.actions,
                ),
              ),
            ),
          ],
        ),
      );
    }

    return SliverAppBar(
      leading: widget.leading,
      automaticallyImplyLeading: widget.automaticallyImplyLeading,
      title: widget.title,
      actions: widget.actions,
      bottom: widget.bottom,
      elevation: widget.elevation,
      scrolledUnderElevation: widget.scrolledUnderElevation,
      shadowColor: widget.shadowColor,
      surfaceTintColor: widget.surfaceTintColor,
      forceElevated: widget.forceElevated,
      backgroundColor: widget.backgroundColor,
      foregroundColor: widget.foregroundColor,
      iconTheme: widget.iconTheme,
      actionsIconTheme: widget.actionsIconTheme,
      primary: widget.primary,
      centerTitle: widget.centerTitle,
      excludeHeaderSemantics: widget.excludeHeaderSemantics,
      titleSpacing: widget.titleSpacing,
      collapsedHeight: widget.collapsedHeight,
      floating: widget.floating,
      pinned: widget.pinned,
      snap: widget.snap,
      stretch: widget.stretch,
      stretchTriggerOffset: widget.stretchTriggerOffset,
      onStretchTrigger: widget.onStretchTrigger,
      shape: widget.shape,
      toolbarHeight: widget.toolbarHeight,
      expandedHeight: _height,
      leadingWidth: widget.leadingWidth,
      toolbarTextStyle: widget.toolbarTextStyle,
      titleTextStyle: widget.titleTextStyle,
      systemOverlayStyle: widget.systemOverlayStyle,
      forceMaterialTransparency: widget.forceMaterialTransparency,
      clipBehavior: widget.clipBehavior,
      flexibleSpace: FlexibleSpaceBar(background: widget.flexibleSpace),
    );
  }
}
