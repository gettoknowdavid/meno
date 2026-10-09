import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// A widget that displays a title bar for a modal.
///
/// The [MModalTitleBar] includes a title and an optional close button.
/// It can be used to provide a consistent header for modal dialogs.
///
/// Example usage:
/// ```dart
/// MModalTitleBar(
///   title: 'Modal Title',
///   showCloseButton: true,
/// )
/// ```
class MModalTitleBar extends StatelessWidget {
  /// Creates an instance of [MModalTitleBar].
  ///
  /// Parameters:
  /// - [key]: An optional key to identify the widget.
  /// - [title]: The title text displayed in the title bar.
  /// - [showCloseButton]: Whether to show the close button. Defaults to true.
  /// - [padding]: Custom padding for the title bar.
  const new({
    required this.title,
    super.key,
    this.showCloseButton = true,
    this.padding,
  });

  /// The title text displayed in the title bar.
  final String title;

  /// Determines whether the close button is displayed.
  ///
  /// Defaults to `true`.
  final bool showCloseButton;

  /// Custom padding for the title bar.
  final EdgeInsetsGeometry? padding;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return Padding(
      padding: padding ?? .zero,
      child: Column(
        mainAxisSize: .min,
        children: [
          SizedBox(
            height: 24,
            child: Row(
              mainAxisAlignment: .spaceBetween,
              children: [
                MText(title, style: textTheme.subheadingMedium),
                if (showCloseButton)
                  IconButton(
                    icon: const Icon(MIcons.x_close),
                    color: colors.onBackgroundVariant,
                    onPressed: () => Navigator.pop(context),
                  ),
              ],
            ),
          ),
          Spaces.verticalSmall,
          const MDivider(),
        ],
      ),
    );
  }
}
