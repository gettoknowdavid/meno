import 'package:flutter_staggered_animations/flutter_staggered_animations.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/chat/model/reaction.dart';
import 'package:meno_mobile/features/chat/widgets/reactions.dart';
import 'package:meno_ui/meno_ui.dart';

/// The floating reaction strip, staggered in from the bottom.
///
/// Sits on the stage beside the room's controls, so a person can react without
/// leaving the broadcast for the chat tab. Each tap goes to [onReaction], which
/// draws it and sends it; the strip itself holds no state, so a room whose
/// socket is between connections still animates what was tapped.
class MReactionButton extends StatelessWidget {
  /// Creates the strip.
  const new({required this.onReaction, super.key});

  /// Called with the reaction a person tapped.
  final ValueChanged<ReactionType> onReaction;

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
          final reaction = reactions[i];

          return InkWell(
            onTap: () => onReaction(reaction.reactionType),
            child: AnimationConfiguration.staggeredList(
              position: i,
              duration: const Duration(milliseconds: 260),
              child: SlideAnimation(
                verticalOffset: 15 + i * 15,
                child: FadeInAnimation(
                  child: MIconButton(
                    size: 40,
                    iconSize: 20,
                    icon: reaction.icon,
                    isFilled: true,
                    fillColor: colors.outlineVariant2,
                    onPressed: () => onReaction(reaction.reactionType),
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
