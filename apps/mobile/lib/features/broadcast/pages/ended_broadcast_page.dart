import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/auth/manager/user_manager.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';

import 'package:meno_ui/meno_ui.dart';

/// The screen a host lands on after their broadcast ends.
///
/// It is reached in two ways: the live shell hands the client here when
/// the room does (`LiveSessionShell` watches `LiveSessionManager.state`),
/// and the editor hands the host here when going live succeeds
/// (`BroadcastEditorPage` chains`createBroadcast` → `startBroadcast`
/// and navigates on the latter's success).
///
/// What it draws comes from the ended broadcast the API minted when
/// it was taken off air — that answer is what
/// [LiveSessionManager.endedBroadcast] holds once the host's `end`
/// call answered, and what the socket's `endedBroadcast` event
/// carries for a listener whose room vanished under them. Both roads
/// ship the same shape, so this page reads from the manager that
/// already has it rather than from a repository method the new
/// implementation does not have.
class EndedBroadcastPage extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final ended = watchValue<LiveSessionManager, EndedBroadcast?>(
      (manager) => manager.endedBroadcast,
    );

    final currentUserId = watchValue<UserManager, Id?>(
      (manager) => manager.currentUserId,
    );

    final isOwnBroadcastEnded =
        ended?.isHostedBy(currentUserId ?? .empty) ?? false;

    Widget? child;

    if (ended == null || ended.isEmpty) {
      child = const _ErrorView(message: 'No broadcast summary available');
    } else {
      child = _PageView(ended: ended, isOwnBroadcastEnded: isOwnBroadcastEnded);
    }

    return PopScope(
      onPopInvokedWithResult: (didPop, result) {
        if (didPop) context.go(MRoutes.home);
      },
      child: child,
    );
  }
}

class _PageView extends StatelessWidget {
  const new({required this.ended, required this.isOwnBroadcastEnded});

  final EndedBroadcast ended;
  final bool isOwnBroadcastEnded;

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);

    final formattedDuration = _formatDuration(ended.duration);

    // The recent joiner avatars are not wired yet: the ended broadcast
    // the API returns carries a total count and a duration, but
    // not the join roster. The widget still gets a stack to draw,
    // so pass an empty list and keep the count.
    final recentParticipants = <Participant>[];

    return MScaffold(
      body: Center(
        child: Column(
          mainAxisSize: .min,
          crossAxisAlignment: .stretch,
          children: [
            Padding(
              padding: const .symmetric(horizontal: 16),
              child: MText(
                'Your live broadcast is complete! Great job.',
                style: textTheme.heading2Bold,
                textAlign: .center,
              ),
            ),
            Spaces.verticalXLarge,
            BroadcastArtworkWidget(image: ended.image),
            Spaces.verticalLarge,
            MTimerDisplay(
              formattedTime: formattedDuration,
              showTimeAgo: false,
              textStyle: textTheme.heading2Bold,
            ),
            Spaces.verticalXLarge,
            EndedBroadcastParticipantsWidget(
              allTimeCount: ended.totalParticipants,
              recentParticipants: recentParticipants,
            ),
            const SizedBox(height: 40),
            MPrimaryButton(
              label: isOwnBroadcastEnded ? 'Publish Broadcast' : 'Listen Back',
              onPressed: () {},
            ),
            Spaces.verticalLarge,
            const _ActionButtons(),
          ],
        ),
      ),
    );
  }
}

/// Turns an [EndedBroadcast.duration] into the HH:MM:SS the timer widget
/// expects. The old summary carried a pre-formatted string; the new
/// one carries a [Duration], so this keeps the screen's contract without
/// adding one to the entity.
String _formatDuration(Duration duration) {
  final hours = duration.inHours.toString().padLeft(2, '0');
  final minutes = (duration.inMinutes % 60).toString().padLeft(2, '0');
  final seconds = (duration.inSeconds % 60).toString().padLeft(2, '0');
  return '$hours:$minutes:$seconds';
}

class _ActionButtons extends StatelessWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        Expanded(
          child: MSecondaryButton(
            label: 'Go Home',
            onPressed: () => context.go(MRoutes.home),
          ),
        ),
        Spaces.horizontalMedium,
        Expanded(
          child: MSecondaryButton(
            label: 'Go to Profile',
            onPressed: () => context.go(MRoutes.myProfile),
          ),
        ),
      ],
    );
  }
}

class _ErrorView extends StatelessWidget {
  const new({this.message});

  final String? message;

  @override
  Widget build(BuildContext context) {
    return MScaffold(
      body: Column(
        mainAxisAlignment: .center,
        children: [
          MErrorState(message: message ?? ''),
          Spaces.verticalLarge,
          const _ActionButtons(),
        ],
      ),
    );
  }
}
