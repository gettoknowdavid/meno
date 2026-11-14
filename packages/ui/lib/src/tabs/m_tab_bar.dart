import 'package:meno_ui/src/tabs/base_tab_bar.dart';
import 'package:meno_ui/src/tabs/meno_tab_type.dart';

/// {@template meno_tab_bar}
/// A concrete implementation of [BaseTabBar] providing named constructors
/// for different tab bar styles.
/// {@endtemplate}
class MTabBar extends BaseTabBar {
  /// Creates a [MTabBar] with the normal tab style.
  ///
  /// The normal style typically features an underline indicator for
  /// the selected tab.
  const new normal({
    required super.tabs,
    required super.controller,
    super.isScrollable = true,
    super.key,
    super.onTap,
    super.padding,
  }) : super(type: MenoTabType.normal);

  /// Creates a [MTabBar] with the contained tab style.
  ///
  /// The contained style displays each tab as a separate, visually
  /// contained element.
  const new contained({
    required super.tabs,
    required super.controller,
    super.isScrollable = true,
    super.key,
    super.onTap,
    super.padding,
  }) : super(type: MenoTabType.contained);
}
