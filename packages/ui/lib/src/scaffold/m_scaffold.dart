import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// A custom scaffold widget that wraps Flutter's Scaffold with additional
/// features.
///
/// This widget provides the same API as Flutter's [Scaffold] but adds a default
/// horizontal padding to the body content and allows for theme integration.
///
/// All parameters match Flutter's [Scaffold] parameters, plus:
/// - [padding]: Custom padding for the body (defaults to horizontal Insets.lg)
///
/// Example usage:
/// ```dart
/// MScaffold(
///   appBar: AppBar(title: Text('Home')),
///   body: Center(child: Text('Hello')),
/// )
/// ```
class MScaffold extends StatelessWidget {
  /// Creates a new `MScaffold` widget.
  ///
  /// All parameters match the standard Flutter [Scaffold] parameters.
  const new({
    super.key,
    this.appBar,
    this.body,
    this.padding = const .symmetric(horizontal: Insets.lg),
    this.resizeToAvoidBottomInset,
    this.primary = true,
    this.drawerScrimColor,
    this.endDrawer,
    this.drawerEdgeDragWidth,
    this.bottomSheet,
    this.floatingActionButton,
    this.floatingActionButtonLocation,
    this.floatingActionButtonAnimator,
    this.persistentFooterButtons,
    this.bottomNavigationBar,
    this.drawer,
  });

  /// The [AppBar] to display at the top of the scaffold.
  final PreferredSizeWidget? appBar;

  /// The main content of the scaffold.
  final Widget? body;

  /// The padding to apply to the body content.
  /// Defaults to `EdgeInsets.symmetric(horizontal: Insets.lg)`.
  final EdgeInsetsGeometry padding;

  /// Whether the scaffold should resize to avoid the bottom inset.
  final bool? resizeToAvoidBottomInset;

  /// Whether the scaffold is the primary focus of the screen.
  final bool primary;

  /// The scrim color for the drawer.
  final Color? drawerScrimColor;

  /// The end drawer widget.
  final Widget? endDrawer;

  /// The width of the drawer edge drag area.
  final double? drawerEdgeDragWidth;

  /// The bottom sheet widget.
  final Widget? bottomSheet;

  /// The floating action button.
  final Widget? floatingActionButton;

  /// The location of the floating action button.
  final FloatingActionButtonLocation? floatingActionButtonLocation;

  /// The animator for the floating action button.
  final FloatingActionButtonAnimator? floatingActionButtonAnimator;

  /// The persistent footer buttons.
  final List<Widget>? persistentFooterButtons;

  /// The bottom navigation bar.
  final Widget? bottomNavigationBar;

  /// The drawer widget.
  final Widget? drawer;

  @override
  Widget build(BuildContext context) => Scaffold(
    key: key,
    appBar: appBar,
    body: body != null ? Padding(padding: padding, child: body) : null,
    resizeToAvoidBottomInset: resizeToAvoidBottomInset,
    primary: primary,
    drawerScrimColor: drawerScrimColor,
    endDrawer: endDrawer,
    drawerEdgeDragWidth: drawerEdgeDragWidth,
    bottomSheet: bottomSheet,
    floatingActionButton: floatingActionButton,
    floatingActionButtonLocation: floatingActionButtonLocation,
    floatingActionButtonAnimator: floatingActionButtonAnimator,
    persistentFooterButtons: persistentFooterButtons,
    bottomNavigationBar: bottomNavigationBar,
    drawer: drawer,
  );
}
