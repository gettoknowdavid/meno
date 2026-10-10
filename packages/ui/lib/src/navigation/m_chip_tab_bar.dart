import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/text/m_text.dart';
import 'package:meno_ui/src/theme/extensions/m_color_scheme.dart';
import 'package:meno_ui/src/theme/extensions/m_text_theme.dart';
import 'package:meno_ui/src/tokens/spacing.dart';

/// The height of a chip, in logical pixels.
const double _kChipHeight = 32;

/// A horizontally scrollable, chip-style tab bar.
///
/// Each [MChipTab] paints its own pill, so the bar needs no custom indicator
/// painter: the selected chip is filled with the theme's primary colour and the
/// rest with the inactive container colour. Selection is read from the shared
/// [TabController], so the chips stay in sync with swipes on an `MTabBarView`.
///
/// Example usage:
/// ```dart
/// MChipTabBar(
///   controller: controller,
///   tabs: const [
///     MChipTab(label: 'All', index: 0, controller: controller),
///     MChipTab(label: 'Live', index: 1, controller: controller),
///   ],
/// )
/// ```
class MChipTabBar extends StatelessWidget implements PreferredSizeWidget {
  /// Creates a chip tab bar.
  ///
  /// The [controller] drives selection and [tabs] are the chips to display.
  const new({required this.tabs, required this.controller, super.key});

  /// The chips shown by the bar, in display order.
  final List<MChipTab> tabs;

  /// The controller that owns the selected index.
  final TabController controller;

  @override
  Size get preferredSize => const Size.fromHeight(_kChipHeight);

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: _kChipHeight,
      child: TabBar(
        controller: controller,
        isScrollable: true,
        tabAlignment: TabAlignment.start,
        indicator: const BoxDecoration(),
        indicatorSize: TabBarIndicatorSize.tab,
        dividerHeight: 0,
        labelPadding: const EdgeInsets.symmetric(horizontal: Insets.sm),
        padding: const EdgeInsets.symmetric(horizontal: Insets.sm),
        labelColor: Colors.transparent,
        unselectedLabelColor: Colors.transparent,
        overlayColor: const WidgetStatePropertyAll<Color>(Colors.transparent),
        tabs: tabs,
      ),
    );
  }
}

/// A single self-painting chip tab.
///
/// Reads its own selected state from [controller] and paints the active or
/// inactive pill, so no shared indicator is required.
class MChipTab extends StatelessWidget {
  /// Creates a chip tab.
  ///
  /// [label] is the chip text, [index] is its position in the [controller], and
  /// [icon] is an optional trailing widget (an asset, emoji, `Icon`, …).
  const new({
    required this.label,
    required this.index,
    required this.controller,
    super.key,
    this.icon,
  });

  /// The text shown inside the chip.
  final String label;

  /// The chip's index within [controller].
  final int index;

  /// The controller that owns the selected index.
  final TabController controller;

  /// An optional trailing widget drawn after [label].
  final Widget? icon;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return AnimatedBuilder(
      animation: controller,
      builder: (context, _) {
        // Use controller.index (settled) rather than the animation value: chip
        // tabs snap on release instead of partially filling during a swipe.
        final isSelected = controller.index == index;

        final background = isSelected
            ? colors.primary
            : colors.inActiveContainer;
        final foreground = isSelected
            ? colors.onPrimary
            : colors.onInActiveContainer;

        return AnimatedContainer(
          duration: const Duration(milliseconds: 200),
          curve: Curves.easeInOut,
          height: _kChipHeight,
          padding: const EdgeInsets.symmetric(horizontal: Insets.lg),
          decoration: BoxDecoration(
            color: background,
            borderRadius: const BorderRadius.all(Radius.circular(100)),
          ),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            spacing: Insets.xs,
            children: [
              MText(label, color: foreground, style: textTheme.captionMedium),
              ?icon,
            ],
          ),
        );
      },
    );
  }
}
