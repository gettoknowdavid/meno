import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:intl/intl.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/_shared/services/permission_prompts.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_ui/meno_ui.dart';

/// The account's upcoming scheduled broadcast, on the home screen.
///
/// # Two states, one card
///
/// Before the reminder window it is a notice with a Hide button: the host knows
/// there is something ahead and can put it away. From five minutes before the
/// start it is a prompt, because a dismissed prompt is a broadcast nobody went
/// live on. Tapping it offers the three things that resolve a schedule: go
/// live, reschedule, or cancel.
///
/// It is absent while a room is open: the live banner is the banner for that,
/// and two cards about the same broadcast would be two answers to one question.
class ScheduledBroadcastBanner extends WatchingStatefulWidget {
  const new({super.key});

  @override
  State<ScheduledBroadcastBanner> createState() =>
      _ScheduledBroadcastBannerState();
}

class _ScheduledBroadcastBannerState extends State<ScheduledBroadcastBanner> {
  /// The schedule whose missed-start sheet has already been presented.
  ///
  /// Held for the life of the home screen so the sheet is offered once per
  /// schedule rather than on every rebuild, and comes back for the next one.
  String? _presentedOverdueFor;

  @override
  Widget build(BuildContext context) {
    final live = watchValue<ActiveBroadcastManager, LiveSnapshot>(
      (manager) => manager.snapshot,
    );
    if (live.isInRoom) return const SizedBox.shrink();

    final manager = di<ScheduledBroadcastManager>();
    // The manager is built lazily and does not read on construction: the screen
    // that draws the banner is what asks.
    callOnce((_) => manager.refresh.run());

    final broadcast = watchValue<ScheduledBroadcastManager, Broadcast?>(
      (m) => m.next,
    );
    if (broadcast == null) return const SizedBox.shrink();

    final isHidden = watchValue<ScheduledBroadcastManager, bool>(
      (m) => m.hidden,
    );
    final isImminent = watchValue<ScheduledBroadcastManager, bool>(
      (m) => m.isImminent,
    );
    if (isHidden && !isImminent) return const SizedBox.shrink();

    final canHide = !isImminent;
    final isDue = watchValue<ScheduledBroadcastManager, bool>((m) => m.isDue);
    final isOverdue = watchValue<ScheduledBroadcastManager, bool>(
      (m) => m.isOverdue,
    );
    final start = broadcast.startTime?.toLocal();

    // A start that has been missed for longer than the grace window is the one
    // moment the app speaks first: the host was probably live on something else
    // and the schedule now needs a decision, not another nudge.
    if (isOverdue && _presentedOverdueFor != broadcast.id.orEmpty) {
      _presentedOverdueFor = broadcast.id.orEmpty;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!mounted || !context.mounted) return;
        unawaited(ScheduledBroadcastActionsModal.show(context, broadcast));
      });
    }

    return _ScheduledBannerCard(
      key: ValueKey('scheduledBroadcastBanner-${broadcast.id.orEmpty}'),
      badgeTitle: isOverdue
          ? 'Missed start'
          : isDue
          ? 'Starting now'
          : 'Scheduled',
      broadcastTitle: broadcast.title.orEmpty,
      when: _whenLabel(start, isDue: isDue, isOverdue: isOverdue),
      onTap: () => ScheduledBroadcastActionsModal.show(context, broadcast),
      action: canHide
          ? MTextButton(
              label: 'Hide',
              onPressed: manager.hide,
              style: TextButton.styleFrom(
                foregroundColor: MColorScheme.of(context).onBackgroundVariant,
              ),
            )
          : null,
    );
  }
}

/// The line under the title: the time, and what can be done about it.
String _whenLabel(
  DateTime? start, {
  required bool isDue,
  required bool isOverdue,
}) {
  if (start == null) return 'Waiting for a start time';
  if (isOverdue) {
    final time = DateFormat('h:mm a').format(start);
    return 'Was due $time · reschedule or cancel';
  }
  if (isDue) return 'Go live now, or pick another time';
  return 'Starts ${DateFormat('EEE, MMM d · h:mm a').format(start)}';
}

class _ScheduledBannerCard extends StatelessWidget {
  const new({
    required this.badgeTitle,
    required this.broadcastTitle,
    required this.when,
    required this.onTap,
    this.action,
    super.key,
  });

  final String badgeTitle;
  final String broadcastTitle;
  final String when;
  final VoidCallback onTap;
  final Widget? action;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return InkWell(
      onTap: onTap,
      borderRadius: .circular(Insets.lg),
      child: Card(
        margin: const .symmetric(horizontal: Insets.lg),
        shape: RoundedSuperellipseBorder(borderRadius: .circular(Insets.lg)),
        child: Padding(
          padding: const .all(14),
          child: Row(
            mainAxisAlignment: .spaceBetween,
            children: [
              Expanded(
                child: Column(
                  mainAxisAlignment: .spaceBetween,
                  crossAxisAlignment: .start,
                  children: [
                    Row(
                      children: [
                        Icon(
                          MIcons.calendar_plus_02,
                          size: 12,
                          color: colors.error,
                        ),
                        Spaces.horizontalMicro,
                        MText(
                          badgeTitle,
                          style: textTheme.microMedium,
                          color: colors.error,
                        ),
                      ],
                    ),
                    const SizedBox(height: 2),
                    Container(
                      height: 24,
                      alignment: .centerLeft,
                      child: MText(
                        broadcastTitle,
                        style: textTheme.captionMedium,
                        maxLines: 1,
                        overflow: .ellipsis,
                      ),
                    ),
                    MText(
                      when,
                      color: colors.onBackgroundVariant,
                      style: textTheme.captionRegular,
                      maxLines: 1,
                      overflow: .ellipsis,
                    ),
                  ],
                ),
              ),
              if (action != null) ...[
                Spaces.horizontalMedium,
                action!,
              ] else
                const Icon(MIcons.chevron_right, size: 20),
            ],
          ),
        ),
      ),
    );
  }
}

/// What a host can do with a schedule that has not started.
///
/// The three answers are the only three there are: keep the promise now, move
/// it, or withdraw it. Dismissing the sheet changes nothing.
class ScheduledBroadcastActionsModal extends StatelessWidget {
  const new({required this.broadcast, super.key});

  final Broadcast broadcast;

  static Future<void> show(BuildContext context, Broadcast broadcast) {
    return showModalBottomSheet<void>(
      context: context,
      useRootNavigator: true,
      isScrollControlled: true,
      builder: (_) => ScheduledBroadcastActionsModal(broadcast: broadcast),
    );
  }

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);
    final start = broadcast.startTime?.toLocal();

    return MModal(
      builder: (context) => Column(
        mainAxisSize: .min,
        children: [
          MText(
            broadcast.title.orEmpty,
            style: textTheme.heading3Medium,
            maxLines: 1,
            overflow: .ellipsis,
          ),
          Spaces.verticalSmall,
          MText(
            start == null
                ? 'Waiting for a start time'
                : DateFormat('EEE, MMM d · h:mm a').format(start),
            style: textTheme.captionRegular,
          ),
          Spaces.verticalXLarge,
          MModalListTile(
            key: const Key('scheduledBroadcast_goLive'),
            leading: const Icon(MIcons.play_circle),
            title: 'Go live now',
            onTap: () => _goLive(context),
          ),
          MModalListTile(
            key: const Key('scheduledBroadcast_reschedule'),
            leading: const Icon(MIcons.calendar_plus_02),
            title: 'Reschedule',
            onTap: () => _reschedule(context),
          ),
          MModalListTile(
            key: const Key('scheduledBroadcast_cancel'),
            leading: const Icon(MIcons.trash),
            title: 'Cancel scheduled broadcast',
            onTap: () => _cancel(context),
          ),
          Spaces.verticalXLarge,
        ],
      ),
    );
  }

  Future<void> _goLive(BuildContext context) async {
    Navigator.of(context).pop();
    // Opening the room is the host speaking, so the microphone is asked for
    // before the entry rather than after it fails.
    if (!await PermissionPrompts.broadcast(context)) return;
    if (!context.mounted) return;

    final live = di<ActiveBroadcastManager>();
    final result = await live.enter(
      LiveEnterRequest(broadcastId: broadcast.id, mode: LiveEnterMode.host),
    );

    if (!context.mounted) return;
    if (!result.isEntered) {
      context.showErrorToast(
        'Could not start this broadcast. Please try again.',
      );
      return;
    }
    await context.push(MRoutes.liveSessionInit);
  }

  Future<void> _reschedule(BuildContext context) async {
    Navigator.of(context).pop();

    final choice = await pickBroadcastStart(
      context,
      initial: broadcast.startTime,
    );
    if (choice == null || !context.mounted) return;

    try {
      await di<ScheduledBroadcastManager>().reschedule.runAsync((
        id: broadcast.id,
        start: choice.start,
        zone: choice.zone,
      ));
      if (!context.mounted) return;
      context.showSuccessToast('Broadcast rescheduled');
    } on Object catch (error) {
      if (!context.mounted) return;
      context.showErrorToast(errorMessage(error));
    }
  }

  Future<void> _cancel(BuildContext context) async {
    Navigator.of(context).pop();

    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: MText(
          'Cancel this broadcast?',
          style: MTextTheme.of(context).heading3Medium,
        ),
        content: MText(
          'It will be removed from your scheduled broadcasts and will not '
          'start automatically. You can schedule another one afterwards.',
          style: MTextTheme.of(context).bodyRegular,
        ),
        actions: [
          MTextButton(
            onPressed: () => Navigator.pop(context, false),
            label: 'Keep it',
          ),
          MTextButton(
            onPressed: () => Navigator.pop(context, true),
            label: 'Cancel broadcast',
            style: TextButton.styleFrom(
              foregroundColor: MColorScheme.of(context).error,
            ),
          ),
        ],
      ),
    );
    if (!(confirmed ?? false) || !context.mounted) return;

    try {
      await di<ScheduledBroadcastManager>().cancel.runAsync(broadcast.id);
      if (!context.mounted) return;
      context.showSuccessToast('Scheduled broadcast cancelled');
    } on Object catch (error) {
      if (!context.mounted) return;
      context.showErrorToast(errorMessage(error));
    }
  }
}
