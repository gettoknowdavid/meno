import 'package:flutter_reaction_button/flutter_reaction_button.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// The reactions the design offers, in the design system's asset pack.
enum ReactionType { clap, flame, like, raise, snap }

/// One reaction: its value and the emoji that draws it.
class MenoReaction {
  const new(this.reactionType, this.icon);

  final ReactionType reactionType;
  final Widget icon;
}

/// Every reaction, for a picker that lists them all.
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

/// The reactions the composer's strip offers.
final mainReactions = <Reaction<ReactionType>>[
  Reaction(
    value: ReactionType.clap,
    icon: Assets.images.clappingHands.image(
      package: 'meno_ui',
      height: 20,
      width: 20,
    ),
  ),
  Reaction(
    value: ReactionType.flame,
    icon: Assets.images.flame.image(package: 'meno_ui', height: 20, width: 20),
  ),
  Reaction(
    value: ReactionType.like,
    icon: Assets.images.redHeart.image(
      package: 'meno_ui',
      height: 20,
      width: 20,
    ),
  ),
  Reaction(
    value: ReactionType.raise,
    icon: Assets.images.raisingHands.image(
      package: 'meno_ui',
      height: 20,
      width: 20,
    ),
  ),
  Reaction(
    value: ReactionType.snap,
    icon: Assets.images.fingerSnap.image(
      package: 'meno_ui',
      height: 20,
      width: 20,
    ),
  ),
];
