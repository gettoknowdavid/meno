import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/gen/assets.gen.dart';
import 'package:meno_ui/src/text/m_text.dart';
import 'package:meno_ui/src/theme/extensions/m_text_theme.dart';
import 'package:meno_ui/src/tokens/spacing.dart';

/// The default size of the empty-state illustration.
const double _kIllustrationSize = 120;

/// A centred empty-state placeholder.
///
/// Shows the Meno illustration with a message underneath, for lists and screens
/// that have no content to display.
///
/// Example usage:
/// ```dart
/// const MEmptyState(title: 'No broadcasts yet')
/// ```
class MEmptyState extends StatelessWidget {
  /// Creates an empty state.
  ///
  /// [title] overrides the default message.
  const new({super.key, this.title});

  /// The message shown below the illustration.
  final String? title;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Column(
        mainAxisAlignment: .center,
        children: [
          Assets.images.liveForYou.image(
            package: 'meno_ui',
            height: _kIllustrationSize,
            width: _kIllustrationSize,
          ),
          Spaces.verticalMedium,
          MText(
            title ?? 'Nothing to show here',
            style: MTextTheme.of(context).captionMedium,
            textAlign: .center,
          ),
        ],
      ),
    );
  }
}
