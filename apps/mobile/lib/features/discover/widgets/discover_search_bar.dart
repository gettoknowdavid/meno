import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_ui/meno_ui.dart';

/// The search field Discover shows in two places.
///
/// On the shell it is [readOnly]: tapping it opens the search screen rather
/// than starting an edit in place, because the results are a screen of
/// their own. On the search screen it is the real field, with a cancel
/// that leaves.
///
/// The controller belongs to the caller — the search screen keeps it for the
/// life of the page — so this widget never owns text it did not create.
class DiscoverSearchBar extends StatelessWidget {
  /// Creates the field.
  ///
  /// [onSubmitted] is called with the typed term when the keyboard asks to
  /// search; [padding] replaces the default gutter.
  const new({
    super.key,
    this.controller,
    this.showCancelButton = false,
    this.readOnly = false,
    this.padding,
    this.onSubmitted,
  });

  /// The text this field shows, when the caller keeps it.
  final TextEditingController? controller;

  /// Called with the term when the keyboard's search action is used.
  final void Function(String)? onSubmitted;

  /// The gutter around the field.
  final EdgeInsetsGeometry? padding;

  /// Whether the field leaves to the search screen when tapped.
  final bool readOnly;

  /// Whether the field is the search screen's own, with a cancel beside it.
  final bool showCancelButton;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return Padding(
      padding: padding ?? const .symmetric(horizontal: Insets.lg),
      child: Row(
        children: [
          Expanded(
            child: SizedBox(
              height: 40,
              child: SearchBar(
                readOnly: readOnly,
                elevation: const WidgetStatePropertyAll(0),
                autoFocus: true,
                onTap: () async {
                  // Already there: tapping the field it is sitting in must not
                  // push a second copy of the screen onto the stack.
                  final location = GoRouter.of(context).state.uri.path;
                  if (location == MRoutes.discoverSearch) return;
                  await context.push(MRoutes.discoverSearch);
                },
                controller: controller,
                onSubmitted: onSubmitted,
                textInputAction: TextInputAction.search,
                hintText: 'Search broadcasts',
                hintStyle: WidgetStatePropertyAll(textTheme.captionRegular),
                padding: const WidgetStatePropertyAll(
                  .symmetric(horizontal: Insets.md),
                ),
                leading: const Icon(MIcons.search, size: Insets.lg),
                shape: const WidgetStatePropertyAll(
                  RoundedRectangleBorder(
                    side: BorderSide(color: Color(0xFFC2C7D0)),
                    borderRadius: Corners.sm,
                  ),
                ),
              ),
            ),
          ),
          if (showCancelButton) ...[
            Spaces.horizontalSmall,
            InkWell(
              onTap: context.pop,
              child: MText(
                'Cancel',
                style: textTheme.captionMedium,
                color: colors.inActive,
              ),
            ),
          ],
        ],
      ),
    );
  }
}
