import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// A tab with nothing behind it yet.
///
/// Every tab under a profile is a feed — recent broadcasts, all broadcasts,
/// favourites — and every one of those belongs to the feature that owns it:
/// broadcasts, favourites, recordings. Until those exist the tab shows the Meno
/// mark, which is honest about there being nothing here, instead of a mock list
/// that would have to be deleted when the real one arrives.
class ProfileTabPlaceholder extends StatelessWidget {
  /// Creates the placeholder.
  ///
  /// [injectOverlap] belongs to the signed-in profile, whose app bar absorbs
  /// the tab's overlap and therefore has to hand the absorbed height back
  /// before the content; the visited profile has no absorber.
  const new({this.injectOverlap = false, super.key});

  /// Whether to inject the height absorbed by the surrounding
  /// [SliverOverlapAbsorber].
  final bool injectOverlap;

  @override
  Widget build(BuildContext context) {
    return CustomScrollView(
      slivers: [
        if (injectOverlap)
          SliverOverlapInjector(
            handle: NestedScrollView.sliverOverlapAbsorberHandleFor(context),
          ),
        const SliverFillRemaining(
          hasScrollBody: false,
          child: Center(child: MPlaceholder(dimension: 96)),
        ),
      ],
    );
  }
}
