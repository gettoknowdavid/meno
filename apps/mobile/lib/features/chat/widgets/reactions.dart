import 'package:flutter_reaction_button/flutter_reaction_button.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/chat/model/reaction.dart';
import 'package:meno_ui/meno_ui.dart';

/// One reaction: its type and the emoji that draws it.
class MenoReaction {
  /// Creates one entry of the pack.
  const new(this.reactionType, this.icon);

  /// Which of the five it is.
  final ReactionType reactionType;

  /// The picture, at the size the pickers draw it.
  final Widget icon;
}

/// Every reaction the room offers, in the design system's asset pack.
final reactions = <MenoReaction>[
  MenoReaction(
    ReactionType.clap,
    Assets.images.clappingHands.image(
      package: 'meno_ui',
      height: 20,
      width: 20,
    ),
  ),
  MenoReaction(
    ReactionType.flame,
    Assets.images.flame.image(package: 'meno_ui', height: 20, width: 20),
  ),
  MenoReaction(
    ReactionType.like,
    Assets.images.redHeart.image(package: 'meno_ui', height: 20, width: 20),
  ),
  MenoReaction(
    ReactionType.raise,
    Assets.images.raisingHands.image(package: 'meno_ui', height: 20, width: 20),
  ),
  MenoReaction(
    ReactionType.snap,
    Assets.images.fingerSnap.image(package: 'meno_ui', height: 20, width: 20),
  ),
];

/// The same five, keyed for the overlay's sprites.
final Map<ReactionType, MenoReaction> _byType = {
  for (final reaction in reactions) reaction.reactionType: reaction,
};

/// The picture for [type], or nothing when this build has no asset for it.
Widget reactionIcon(ReactionType type) =>
    _byType[type]?.icon ?? const SizedBox.shrink();

/// The reactions the composers' strips offer, built from the same five.
final mainReactions = <Reaction<ReactionType>>[
  for (final reaction in reactions)
    Reaction(value: reaction.reactionType, icon: reaction.icon),
];
