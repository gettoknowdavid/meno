import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/scheduler.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/chat/manager/reactions_manager.dart';
import 'package:meno_mobile/features/chat/model/reaction.dart';
import 'package:meno_mobile/features/chat/widgets/reactions.dart';

/// The room's reactions, floating up over the stage.
///
/// # What it draws
///
/// Every burst the room's [ReactionsManager] announces, local and remote, as a
/// sprite that rises from the foot of the stage, sways once on the way and
/// fades out. Nothing here holds state the manager does not: a sprite exists
/// from its burst until its lifetime ends, and the loop stops when the last one
/// leaves.
///
/// # Why a ticker and not a controller per sprite
///
/// One [Ticker] drives every sprite, so a busy room costs one frame callback
/// rather than one animation controller per tap. Positions are a function of
/// the sprite's age, which also makes them cheap to recompute: no implied
/// animation state to keep in step with a rebuild.
///
/// # It cannot be touched
///
/// The layer sits over the stage's controls, so it ignores pointers entirely. A
/// reaction should never be the reason a microphone button misses a tap.
///
/// # Where its state comes from
///
/// Not `watchStream`: each burst is an event that starts a sprite with its own
/// age, not the current value of anything. The widget keeps the list and the
/// ticker; the room's manager owns sending and filtering, and the page around
/// it is what notices a session appearing.
class ReactionOverlay extends StatefulWidget {
  /// Creates the overlay for [reactions].
  const new({required this.reactions, super.key});

  /// The room's reactions.
  final ReactionsManager reactions;

  @override
  State<ReactionOverlay> createState() => _ReactionOverlayState();
}

class _ReactionOverlayState extends State<ReactionOverlay>
    with SingleTickerProviderStateMixin {
  /// How many sprites may share the stage. Older ones make room for newer.
  static const _maxSprites = 28;

  /// How many columns the sprites are spread across.
  static const _lanes = 6;

  /// How long one sprite lives.
  static const _lifetime = Duration(milliseconds: 3200);

  /// Where the lane band starts, as a fraction of the width: up the right side,
  /// out of the way of the room's controls and its titles.
  static const _laneStart = 0.62;

  static const _laneStep = 0.055;
  static const _bottomInset = 8.0;
  static const _spriteSize = 32.0;

  final _sprites = <_Sprite>[];
  final _random = math.Random();

  StreamSubscription<ReactionBurst>? _subscription;
  Ticker? _ticker;
  Duration _elapsed = Duration.zero;
  int _nextLane = 0;
  bool _stopScheduled = false;

  @override
  void initState() {
    super.initState();
    _ticker = createTicker(_onTick);
    _subscription = widget.reactions.bursts.listen(_onBurst);
  }

  @override
  void didUpdateWidget(ReactionOverlay oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.reactions == widget.reactions) return;

    unawaited(_subscription?.cancel());
    _subscription = widget.reactions.bursts.listen(_onBurst);
  }

  @override
  void dispose() {
    unawaited(_subscription?.cancel());
    _subscription = null;
    _ticker?.dispose();
    _ticker = null;
    super.dispose();
  }

  void _onBurst(ReactionBurst burst) {
    if (!mounted) return;

    final ticker = _ticker;
    if (ticker != null && !ticker.isActive) {
      // A ticker starts its clock at zero, so a fresh run has to begin the
      // sprites' timestamps from zero with it.
      _elapsed = Duration.zero;
      ticker.start();
    }

    setState(() {
      _sprites.add(
        _Sprite(
          type: burst.type,
          spawnedAt: _elapsed,
          lane: _nextLane++ % _lanes,
          jitter: _random.nextDouble(),
          scale: 0.85 + _random.nextDouble() * 0.35,
        ),
      );
      if (_sprites.length > _maxSprites) {
        _sprites.removeRange(0, _sprites.length - _maxSprites);
      }
    });
  }

  void _onTick(Duration elapsed) {
    _elapsed = elapsed;
    if (!mounted) return;

    if (_sprites.isNotEmpty) {
      setState(() {
        _sprites.removeWhere(
          (sprite) => elapsed - sprite.spawnedAt >= _lifetime,
        );
      });
    }

    if (_sprites.isEmpty) _stopTicker();
  }

  /// Stops the loop once the stage is clear, so an idle room costs no frames.
  ///
  /// The stop runs after the frame, never inside the tick that asked for it:
  /// stopping a ticker from its own callback cancels the frame being delivered
  /// and leaves the scheduler waiting on a callback that will not come.
  void _stopTicker() {
    final ticker = _ticker;
    if (ticker == null || !ticker.isActive || _stopScheduled) return;

    _stopScheduled = true;
    SchedulerBinding.instance.addPostFrameCallback((_) {
      _stopScheduled = false;
      if (!mounted || _sprites.isNotEmpty) return;
      ticker.stop();
    });
  }

  @override
  Widget build(BuildContext context) {
    if (_sprites.isEmpty) return const SizedBox.shrink();

    // Motion is what this layer is; a person who asked the system to reduce it
    // gets only the fade, and still sees the reactions.
    final still = MediaQuery.disableAnimationsOf(context);
    final travel = still ? 0.1 : 0.72;

    return IgnorePointer(
      child: LayoutBuilder(
        builder: (context, constraints) {
          final width = constraints.maxWidth;
          final height = constraints.maxHeight;

          return Stack(
            clipBehavior: Clip.none,
            children: [
              for (final sprite in _sprites)
                _positioned(
                  sprite,
                  width: width,
                  height: height,
                  travel: travel,
                ),
            ],
          );
        },
      ),
    );
  }

  Widget _positioned(
    _Sprite sprite, {
    required double width,
    required double height,
    required double travel,
  }) {
    final age = _elapsed - sprite.spawnedAt;
    final progress = (age.inMicroseconds / _lifetime.inMicroseconds).clamp(
      0.0,
      1.0,
    );

    final sway = math.sin(progress * math.pi * 2.2 + sprite.lane) * 8;
    final left =
        width * (_laneStart + sprite.lane * _laneStep) +
        sprite.jitter * 16 +
        sway;

    // A sprite is never drawn off the edge: the lane band and the sway can both
    // push it past the width on a narrow stage.
    final maxLeft = math.max(0, width - _spriteSize).toDouble();

    return Positioned(
      left: left.clamp(0, maxLeft).toDouble(),
      bottom: _bottomInset + progress * travel * height,
      child: FloatingReaction(
        type: sprite.type,
        scale: sprite.scale + progress * 0.35,
        opacity: _opacityAt(progress),
      ),
    );
  }
}

/// How visible a sprite is at [progress] of its flight.
///
/// It fades in over the first tenth, holds while it climbs, then fades out over
/// the last third — so a reaction arrives and leaves without ever popping.
double _opacityAt(double progress) {
  if (progress < 0.1) return progress / 0.1;
  if (progress <= 0.62) return 1;
  return (1 - progress) / 0.38;
}

/// One reaction's picture, at one moment of its flight.
class FloatingReaction extends StatelessWidget {
  /// Creates the picture for [type].
  const new({
    required this.type,
    required this.scale,
    required this.opacity,
    super.key,
  });

  /// Which reaction it is.
  final ReactionType type;

  /// How large it is drawn, relative to its asset.
  final double scale;

  /// How opaque it is right now.
  final double opacity;

  @override
  Widget build(BuildContext context) => Opacity(
    opacity: opacity,
    child: Transform.scale(scale: scale, child: reactionIcon(type)),
  );
}

/// One sprite's immutable description; where it is depends on its age.
final class _Sprite {
  const new({
    required this.type,
    required this.spawnedAt,
    required this.lane,
    required this.jitter,
    required this.scale,
  });

  final ReactionType type;

  /// The ticker time the burst arrived at.
  final Duration spawnedAt;

  /// Which column it rises in, so two reactions from one moment do not overlap.
  final int lane;

  /// A stable 0..1 offset within its lane, so repeats of one lane are not a
  /// straight line.
  final double jitter;

  /// The base size this sprite is drawn at.
  final double scale;
}
