import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// A widget that displays a modal dialog with an optional title bar.
///
/// The [MModal] provides a consistent modal structure with a title bar
/// that includes a close button. It can be used as a base for building
/// various modal dialogs throughout the app.
///
/// Example usage:
/// ```dart
/// MModal(
///   title: 'Modal Title',
///   builder: (context) => Center(child: Text('Content')),
/// )
/// ```
class MModal extends StatelessWidget {
  /// Creates an instance of [MModal].
  ///
  /// Parameters:
  /// - [key]: An optional key to identify the widget.
  /// - [builder]: The WidgetBuilder for the modal content.
  /// - [title]: An optional title for the modal.
  /// - [showCloseButton]: Whether to show the close button. Defaults to true.
  /// - [padding]: Custom padding for the modal.
  const new({
    required this.builder,
    super.key,
    this.title,
    this.showCloseButton = true,
    this.padding,
  });

  /// WidgetBuilder for the modal content.
  final WidgetBuilder builder;

  /// The title text displayed in the title bar.
  final String? title;

  /// Determines whether the close button is displayed.
  ///
  /// Defaults to `true`.
  final bool showCloseButton;

  /// Custom padding for the modal.
  final EdgeInsetsGeometry? padding;

  /// Shows the modal using [showDialog].
  ///
  /// Returns a [Future] that completes when the modal is closed.
  static Future<T?> show<T>({
    required BuildContext context,
    required WidgetBuilder builder,
    String? title,
    bool showCloseButton = true,
    EdgeInsetsGeometry? padding,
  }) {
    return showDialog<T>(
      context: context,
      builder: (context) => MModal(
        builder: builder,
        title: title,
        showCloseButton: showCloseButton,
        padding: padding,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final effectiveContentPadding = title != null
        ? const EdgeInsets.only(top: 48, bottom: 8)
        : const EdgeInsets.only(bottom: 8);

    return SafeArea(
      child: Container(
        width: MediaQuery.sizeOf(context).width,
        padding: padding ?? const .fromLTRB(16, 0, 16, 16),
        child: Stack(
          children: [
            if (title != null)
              Positioned.fill(
                child: MModalTitleBar(
                  title: title!,
                  showCloseButton: showCloseButton,
                ),
              ),
            Padding(padding: effectiveContentPadding, child: builder(context)),
          ],
        ),
      ),
    );
  }
}
