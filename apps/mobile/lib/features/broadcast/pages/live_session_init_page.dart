import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_ui/meno_ui.dart';

/// The transition into a live room.
///
/// It reads one value — [ActiveBroadcastManager.snapshot] — and shows whichever
/// surface that phase calls for. There is no scope to wait for, no readiness
/// signal to await and no listener bridged to a future: the session either
/// exists, is connecting, failed, or has ended, and each of those is a phase
/// (audit M21).
///
/// It is a page rather than a dialog because it is a *destination*: a deep link
/// that resumed a session lands here, and a failure has to leave the user
/// somewhere with a way forward.
class LiveSessionInitPage extends WatchingStatefulWidget {
  const new({this.broadcastId, super.key});

  /// The room this transition is for, when the route knows it.
  ///
  /// Only used as a fallback for the ended screen: the snapshot's retained
  /// summary is the authority once there is one.
  final Id? broadcastId;

  @override
  State<LiveSessionInitPage> createState() => _LiveSessionInitPageState();
}

class _LiveSessionInitPageState extends State<LiveSessionInitPage> {
  /// Whether this page has already handed off to another screen.
  ///
  /// A frame can rebuild while the navigation is still settling; without this
  /// the `live` phase would push the room again on every rebuild.
  bool _handedOff = false;

  @override
  Widget build(BuildContext context) {
    final manager = di<ActiveBroadcastManager>();

    final snapshot = watchValue<ActiveBroadcastManager, LiveSnapshot>(
      (manager) => manager.snapshot,
    );

    switch (snapshot.phase) {
      case LivePhase.connecting:
        return const _ProgressView(message: 'Connecting to the broadcast…');

      case LivePhase.reconnecting:
        return const _ProgressView(message: 'Reconnecting to the broadcast…');

      case LivePhase.ending:
        return const _ProgressView(message: 'Ending the broadcast…');

      case LivePhase.live:
        _handOff(() => context.replace(MRoutes.liveBroadcast));
        return const _ProgressView(message: 'Joining the room…');

      case LivePhase.failed:
        return _FailureView(
          failure: snapshot.failure,
          onRetry: () => unawaited(manager.retry()),
        );

      case LivePhase.ended:
        final endedId = snapshot.endedSummary?.id ?? widget.broadcastId;
        if (endedId != null && endedId.isValid) {
          _handOff(() => context.replace(MRoutes.endedBroadcastRoute(endedId)));
        } else {
          _handOff(() => context.go(MRoutes.home));
        }
        return const _ProgressView(message: 'Closing the broadcast…');

      case LivePhase.idle:
        // Nothing is open, so there is no room to enter: the flow leaves
        // rather than waiting for a session that will never arrive.
        _handOff(() => context.go(MRoutes.home));
        return const _ProgressView(message: 'No broadcast is open.');
    }
  }

  /// Navigates once, after the frame that showed the phase has settled.
  void _handOff(VoidCallback action) {
    if (_handedOff) return;
    _handedOff = true;

    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) action();
    });
  }
}

/// The wait, and what an un-navigated frame shows while it happens.
class _ProgressView extends StatelessWidget {
  const new({required this.message});

  final String message;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: Center(
        child: Column(
          mainAxisAlignment: .center,
          children: [
            const MLoadingIndicator(100, 100),
            Spaces.verticalLarge,
            MText(message),
          ],
        ),
      ),
    );
  }
}

/// What to do when the room cannot be entered.
class _FailureView extends StatelessWidget {
  const new({required this.failure, required this.onRetry});

  final LiveFailure? failure;
  final VoidCallback onRetry;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: Center(
        child: Column(
          crossAxisAlignment: .stretch,
          mainAxisAlignment: .center,
          children: [
            MErrorState(
              message:
                  failure?.message ??
                  'The broadcast could not be entered. Please try again.',
              // A refused token will be refused again; a dropped connection may
              // not be. Only the retryable failure offers the button.
              onRetry: failure?.retryable ?? true ? onRetry : null,
            ),
            Spaces.verticalLarge,
            Row(
              mainAxisAlignment: .center,
              spacing: 8,
              children: [
                MTextButton(
                  label: 'Go Home',
                  onPressed: () => context.go(MRoutes.home),
                ),
                MTextButton(
                  label: 'Back to Broadcast Editor',
                  onPressed: () async {
                    context.go(MRoutes.home);
                    await context.push(MRoutes.broadcastEditor);
                  },
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
