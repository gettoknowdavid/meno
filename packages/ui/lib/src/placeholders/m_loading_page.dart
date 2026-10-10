import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/loading_indicator/m_loading_indicator.dart';
import 'package:meno_ui/src/scaffold/m_scaffold.dart';
import 'package:meno_ui/src/text/m_text.dart';
import 'package:meno_ui/src/theme/extensions/m_text_theme.dart';
import 'package:meno_ui/src/tokens/spacing.dart';

/// A full-screen loading screen.
///
/// The loading sibling of `MEmptyState` and `MErrorState`: it fills the screen
/// with the design system's adaptive loading indicator on the themed
/// background, for the moments before a route has anything to show.
///
/// `MLoadingIndicator.box` already centres itself, so no extra [Center] is
/// needed — the column below only adds the optional [message].
///
/// Example usage:
/// ```dart
/// const MLoadingPage()
/// ```
/// ```dart
/// const MLoadingPage(message: 'Starting your live session…')
/// ```
class MLoadingPage extends StatelessWidget {
  /// Creates a loading page.
  ///
  /// [message] is an optional line shown under the indicator, for when the wait
  /// needs a little context.
  const new({super.key, this.message});

  /// An optional message shown under the indicator.
  final String? message;

  @override
  Widget build(BuildContext context) {
    return MScaffold(
      body: Column(
        mainAxisAlignment: .center,
        children: [
          const MLoadingIndicator.box(),
          if (message != null) ...[
            Spaces.verticalLarge,
            MText(
              message!,
              style: MTextTheme.of(context).captionMedium,
              textAlign: .center,
            ),
          ],
        ],
      ),
    );
  }
}
