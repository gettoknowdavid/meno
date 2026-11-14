import 'package:flutter_staggered_animations/flutter_staggered_animations.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/chat/widgets/reactions.dart';
import 'package:meno_ui/meno_ui.dart';

/// The floating reaction strip, staggered in from the bottom.
///
/// The reactions are drawn but not yet wired to the API's reaction frames:
/// this is the design's strip, kept here so the room looks as specified.
class MReactionButton extends StatelessWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);

    return Container(
      height: 56,
      padding: const .all(Insets.sm),
      decoration: BoxDecoration(
        color: colors.background,
        borderRadius: Corners.circle,
      ),
      child: ListView.separated(
        shrinkWrap: true,
        separatorBuilder: (context, i) => Spaces.horizontalSmall,
        scrollDirection: .horizontal,
        itemCount: reactions.length,
        itemBuilder: (context, i) {
          return InkWell(
            onTap: () {},
            child: AnimationConfiguration.staggeredList(
              position: i,
              duration: const Duration(milliseconds: 260),
              child: SlideAnimation(
                verticalOffset: 15 + i * 15,
                child: FadeInAnimation(
                  child: MIconButton(
                    size: 40,
                    iconSize: 20,
                    icon: reactions[i].icon,
                    isFilled: true,
                    fillColor: colors.outlineVariant2,
                  ),
                ),
              ),
            ),
          );
        },
      ),
    );
  }
}
