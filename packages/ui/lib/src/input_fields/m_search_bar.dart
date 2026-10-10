import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/theme/extensions/m_color_scheme.dart';
import 'package:meno_ui/src/theme/extensions/m_text_theme.dart';
import 'package:meno_ui/src/tokens/corners.dart';
import 'package:meno_ui/src/tokens/m_icons.dart';
import 'package:meno_ui/src/tokens/spacing.dart';

/// A themed search field.
///
/// Wraps Material's [SearchBar] with the design system's colours, corner radius
/// and border states, so a search field matches the rest of the app without
/// repeated styling.
///
/// Example usage:
/// ```dart
/// MSearchBar(
///   hintText: 'Search broadcasts',
///   onChanged: onQueryChanged,
/// )
/// ```
class MSearchBar extends StatelessWidget {
  /// Creates a search bar.
  ///
  /// [hintText] is shown while the field is empty. [leading] and [trailing]
  /// replace the default magnifier and the trailing actions respectively.
  const new({
    required this.hintText,
    super.key,
    this.focusNode,
    this.onChanged,
    this.leading,
    this.trailing,
    this.enabled = true,
  });

  /// The hint shown while the field is empty.
  final String hintText;

  /// The node controlling focus for the field.
  final FocusNode? focusNode;

  /// Called whenever the query changes.
  final void Function(String)? onChanged;

  /// An optional widget drawn before the input, defaulting to a magnifier.
  final Widget? leading;

  /// Optional widgets drawn after the input.
  final Iterable<Widget>? trailing;

  /// Whether the field accepts input.
  final bool enabled;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return SearchBar(
      focusNode: focusNode,
      scrollPadding: EdgeInsets.zero,
      hintText: hintText,
      onChanged: onChanged,
      elevation: const WidgetStatePropertyAll<double>(0),
      enabled: enabled,
      trailing: trailing,
      leading: leading ?? const Icon(MIcons.search, size: Insets.lg),
      constraints: BoxConstraints.tight(const Size.fromHeight(40)),
      padding: const WidgetStatePropertyAll<EdgeInsets>(
        EdgeInsets.symmetric(horizontal: Insets.md),
      ),
      hintStyle: WidgetStatePropertyAll<TextStyle>(textTheme.captionRegular),
      shape: const WidgetStatePropertyAll<OutlinedBorder>(
        RoundedRectangleBorder(borderRadius: Corners.sm),
      ),
      side: WidgetStateProperty.resolveWith((states) {
        final side = BorderSide(color: colors.inActive);
        if (states.contains(WidgetState.error)) {
          return side.copyWith(color: colors.error, width: 2);
        } else if (states.contains(WidgetState.disabled)) {
          return side.copyWith(color: colors.disabled);
        } else if (states.contains(WidgetState.focused)) {
          return side.copyWith(color: colors.primary, width: 2);
        } else if (states.contains(WidgetState.selected)) {
          return side.copyWith(color: colors.primary, width: 2);
        }
        return side;
      }),
      textStyle: WidgetStateTextStyle.resolveWith((states) {
        final baseTextStyle = textTheme.captionRegular;
        late Color color;
        if (states.contains(WidgetState.disabled)) {
          color = colors.inActive;
        } else if (states.contains(WidgetState.focused)) {
          color = colors.onBackground;
        } else if (states.contains(WidgetState.error)) {
          color = colors.onBackground;
        } else {
          color = colors.inActive;
        }
        return baseTextStyle.copyWith(color: color);
      }),
    );
  }
}
