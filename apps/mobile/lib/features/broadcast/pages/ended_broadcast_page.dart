import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_ui/meno_ui.dart';

/// The screen shown after a broadcast ends.
///
/// It is addressed by its broadcast id, so it works after the session that
/// ended it is gone — including a process-level termination that removed the
/// room entirely. The manager's retained summary is used when it is this
/// broadcast's; otherwise the page loads the broadcast by id with a manager it
/// owns itself (audit H04).
class EndedBroadcastPage extends WatchingWidget {
  const new({required this.broadcastId, super.key});

  /// The broadcast this screen is about.
  final Id broadcastId;

  @override
  Widget build(BuildContext context) {
    final snapshot = watchValue<ActiveBroadcastManager, LiveSnapshot>(
      (manager) => manager.snapshot,
    );

    final summary = snapshot.endedSummary;

    final accountId = di<ActiveBroadcastManager>().accountId;

    final retained = summary?.id == broadcastId ? summary : null;

    Widget child;

    if (retained != null) {
      child = _PageView(
        broadcast: retained,
        isOwnBroadcastEnded: retained.isHostedBy(accountId),
      );
    } else {
      // The ended screen only needs the broadcast's own record — duration,
      // participants, artwork — so it reads it directly by id rather than
      // through a proxy built for a live details page.
      final load = createOnce(
        () => ValueNotifier<Future<Broadcast>>(
          di<BroadcastHttpService>().getBroadcast(broadcastId),
        ),
      );

      final request = watchFuture<ValueNotifier<Future<Broadcast>>, Broadcast>(
        (notifier) => notifier.value,
        target: load,
        initialValue: Broadcast.empty,
        allowFutureChange: true,
      );

      if (request.hasError) {
        // The id names nothing this account may see: a typed refusal, not an
        // empty screen that waits forever.
        child = const _ErrorView(message: 'This broadcast could not be found.');
      } else if (request.connectionState != .done) {
        child = const MScaffold(body: Center(child: MLoadingIndicator(80, 80)));
      } else {
        final broadcast = request.data!;
        child = _PageView(
          broadcast: broadcast,
          isOwnBroadcastEnded: broadcast.isHostedBy(accountId),
        );
      }
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
  const new({required this.broadcast, required this.isOwnBroadcastEnded});

  final Broadcast broadcast;
  final bool isOwnBroadcastEnded;

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);

    final formattedDuration = _formatDuration(
      broadcast.duration ?? Duration.zero,
    );

    // The recent joiner avatars are not wired yet: the ended broadcast the API
    // returns carries a total count and a duration, but not the join roster.
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
            BroadcastArtworkWidget(image: broadcast.image),
            Spaces.verticalLarge,
            MTimerDisplay(
              formattedTime: formattedDuration,
              showTimeAgo: false,
              textStyle: textTheme.heading2Bold,
            ),
            Spaces.verticalXLarge,
            EndedBroadcastParticipantsWidget(
              allTimeCount: broadcast.totalParticipants,
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

/// Turns a [Broadcast.duration] into the HH:MM:SS the timer widget expects.
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
