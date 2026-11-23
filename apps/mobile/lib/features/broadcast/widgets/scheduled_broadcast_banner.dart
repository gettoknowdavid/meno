import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:intl/intl.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/_shared/widgets/permission_prompts.dart';
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
/// The four answers are: keep the promise now, correct it, move it, or
/// withdraw it. Dismissing the sheet changes nothing.
///
/// Every action closes the sheet first and then continues on [hostContext].
/// The sheet's own context is gone the moment it pops, so a picker, dialog,
/// permission prompt or navigation started from it either did nothing or
/// stopped after its first step — which is exactly what a tap on Go live or
/// Cancel used to do.
class ScheduledBroadcastActionsModal extends StatelessWidget {
  const new({
    required this.broadcast,
    required this.hostContext,
    super.key,
  });

  final Broadcast broadcast;

  /// The context of the screen that opened the sheet, used for everything the
  /// action does after the sheet is gone.
  final BuildContext hostContext;

  static Future<void> show(BuildContext context, Broadcast broadcast) {
    return showModalBottomSheet<void>(
      context: context,
      useRootNavigator: true,
      isScrollControlled: true,
      builder: (_) => ScheduledBroadcastActionsModal(
        broadcast: broadcast,
        hostContext: context,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);
    final start = broadcast.startTime?.toLocal();
    final description = broadcast.description?.orEmpty.trim() ?? '';

    return MModal(
      builder: (context) => Column(
        mainAxisSize: .min,
        children: [
          BroadcastArtworkWidget(image: broadcast.image, radius: 40),
          BroadcastTitleWidget(
            key: const Key('scheduledBroadcast_title'),
            title: broadcast.title.orEmpty,
          ),
          Spaces.verticalMicro,
          MText(
            start == null
                ? 'Waiting for a start time'
                : DateFormat('EEE, MMM d · h:mm a').format(start),
            style: textTheme.captionRegular,
            textAlign: .center,
          ),
          if (description.isNotEmpty) ...[
            Spaces.verticalSmall,
            Padding(
              padding: const .symmetric(horizontal: Insets.lg),
              child: MText(
                description,
                style: textTheme.captionRegular,
                color: MColorScheme.of(context).onBackgroundVariant,
                textAlign: .center,
                maxLines: 2,
                overflow: .ellipsis,
              ),
            ),
          ],
          Spaces.verticalXLarge,
          MModalListTile(
            key: const Key('scheduledBroadcast_goLive'),
            leading: const Icon(MIcons.play_circle),
            title: 'Go live now',
            onTap: () => _goLive(context),
          ),
          MModalListTile(
            key: const Key('scheduledBroadcast_edit'),
            leading: const Icon(MIcons.edit_02),
            title: 'Edit broadcast details',
            onTap: () => _edit(context),
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

    final host = hostContext;
    if (!host.mounted) return;

    // Opening the room is the host speaking, so the microphone is asked for
    // before the entry rather than after it fails.
    if (!await PermissionPrompts.broadcast(host)) return;
    if (!host.mounted) return;

    final live = di<ActiveBroadcastManager>();
    final result = await live.enter(
      LiveEnterRequest(broadcastId: broadcast.id, mode: LiveEnterMode.host),
    );

    // The row is no longer waiting for its moment, so the home banner and the
    // scheduled tab both have one less thing to offer.
    if (di.isRegistered<ScheduledBroadcastManager>()) {
      di<ScheduledBroadcastManager>().refresh.run();
    }

    if (!host.mounted) return;
    if (!result.isEntered) {
      host.showErrorToast('Could not start this broadcast. Please try again.');
      return;
    }
    await host.push(MRoutes.liveSessionInit);
  }

  Future<void> _edit(BuildContext context) async {
    Navigator.of(context).pop();

    final host = hostContext;
    if (!host.mounted) return;

    await ScheduledBroadcastEditModal.show(host, broadcast);
  }

  Future<void> _reschedule(BuildContext context) async {
    Navigator.of(context).pop();

    final host = hostContext;
    if (!host.mounted) return;

    // Date and time: the picker asks for both, and the sheet is out of the way
    // so the dialogs sit on the screen that opened it.
    final choice = await pickBroadcastStart(host, initial: broadcast.startTime);
    if (choice == null || !host.mounted) return;

    try {
      await di<ScheduledBroadcastManager>().reschedule.runAsync((
        id: broadcast.id,
        start: choice.start,
        zone: choice.zone,
      ));
      if (!host.mounted) return;
      host.showSuccessToast('Broadcast rescheduled');
    } on Object catch (error) {
      if (!host.mounted) return;
      host.showErrorToast(errorMessage(error));
    }
  }

  Future<void> _cancel(BuildContext context) async {
    Navigator.of(context).pop();

    final host = hostContext;
    if (!host.mounted) return;

    final confirmed = await showDialog<bool>(
      context: host,
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
    if (!(confirmed ?? false) || !host.mounted) return;

    try {
      await di<ScheduledBroadcastManager>().cancel.runAsync(broadcast.id);
      if (!host.mounted) return;
      host.showSuccessToast('Scheduled broadcast cancelled');
    } on Object catch (error) {
      if (!host.mounted) return;
      host.showErrorToast(errorMessage(error));
    }
  }
}

/// The details of a schedule, editable in place.
///
/// Title, description and start time are what a card draws and what the host
/// can get wrong before the moment arrives; fixing a typo should not mean
/// cancelling the promise and making a new one. The artwork is shown so the
/// sheet is recognisably the same broadcast, and the start time opens the same
/// date-then-time picker the editor uses.
class ScheduledBroadcastEditModal extends StatefulWidget {
  const new({required this.broadcast, super.key});

  final Broadcast broadcast;

  static Future<void> show(BuildContext context, Broadcast broadcast) {
    return showModalBottomSheet<void>(
      context: context,
      useRootNavigator: true,
      isScrollControlled: true,
      builder: (_) => ScheduledBroadcastEditModal(broadcast: broadcast),
    );
  }

  @override
  State<ScheduledBroadcastEditModal> createState() =>
      _ScheduledBroadcastEditModalState();
}

class _ScheduledBroadcastEditModalState
    extends State<ScheduledBroadcastEditModal> {
  late final TextEditingController _title;
  late final TextEditingController _description;
  late DateTime? _start;
  String? _zone;
  bool _saving = false;

  @override
  void initState() {
    super.initState();
    _title = TextEditingController(text: widget.broadcast.title.orEmpty);
    _description = TextEditingController(
      text: widget.broadcast.description?.orEmpty ?? '',
    );
    _start = widget.broadcast.startTime?.toLocal();
    _zone = widget.broadcast.timeZone;
  }

  @override
  void dispose() {
    _title.dispose();
    _description.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);
    final start = _start;
    final when = start == null
        ? 'Choose a start time'
        : '${DateFormat("EEE, MMM d 'at' h:mm a").format(start)}'
              '${_zone == null ? '' : ' · $_zone'}';

    return MModal(
      title: 'Edit broadcast',
      builder: (context) => SingleChildScrollView(
        padding: EdgeInsets.only(
          bottom: MediaQuery.viewInsetsOf(context).bottom,
        ),
        child: Column(
          crossAxisAlignment: .stretch,
          mainAxisSize: .min,
          children: [
            BroadcastArtworkWidget(image: widget.broadcast.image, radius: 40),
            Spaces.verticalSmall,
            MTextFormField(
              key: const Key('scheduledBroadcastEdit_title'),
              label: 'Broadcast title',
              hint: "Jim Halpert's live audio",
              controller: _title,
              enabled: !_saving,
            ),
            Spaces.verticalLarge,
            MTextArea(
              key: const Key('scheduledBroadcastEdit_description'),
              label: 'About broadcast',
              hint: 'Enter a brief description',
              maxLines: 4,
              maxLength: 244,
              controller: _description,
              enabled: !_saving,
            ),
            Spaces.verticalLarge,
            MModalListTile(
              key: const Key('scheduledBroadcastEdit_startTime'),
              leading: const Icon(MIcons.calendar_plus_02),
              title: when,
              trailing: MIcons.edit_02,
              onTap: _saving ? null : () => unawaited(_pickStart(context)),
            ),
            Spaces.verticalXLarge,
            MPrimaryButton(
              key: const Key('scheduledBroadcastEdit_save'),
              label: 'Save changes',
              loading: _saving,
              onPressed: _saving ? null : () => unawaited(_save()),
            ),
            Spaces.verticalLarge,
            MText(
              'The cover is kept as it is.',
              style: textTheme.microRegular,
              color: MColorScheme.of(context).onBackgroundVariant,
              textAlign: .center,
            ),
            Spaces.verticalLarge,
          ],
        ),
      ),
    );
  }

  Future<void> _pickStart(BuildContext context) async {
    final choice = await pickBroadcastStart(context, initial: _start);
    if (choice == null || !mounted) return;

    setState(() {
      _start = choice.start;
      _zone = choice.zone ?? _zone;
    });
  }

  Future<void> _save() async {
    final title = _title.text.trim();
    if (title.isEmpty) {
      context.showErrorToast('A broadcast needs a title.');
      return;
    }

    final description = _description.text.trim();
    final originalDescription = (widget.broadcast.description?.orEmpty ?? '')
        .trim();
    final start = _start;
    final original = widget.broadcast.startTime;
    final startChanged =
        start != null &&
        (original == null ||
            start.toUtc().difference(original.toUtc()).inSeconds.abs() > 60);

    // Only what changed travels: a patch with a start time in the past is
    // refused, and a schedule that has already missed its moment is exactly
    // the row whose title a host may still want to correct.
    final titleChanged = title != widget.broadcast.title.orEmpty.trim();
    final descriptionChanged =
        description.isNotEmpty && description != originalDescription;

    if (!titleChanged && !descriptionChanged && !startChanged) {
      context.showSuccessToast('Nothing to update');
      Navigator.of(context).pop();
      return;
    }

    setState(() => _saving = true);

    try {
      await di<ScheduledBroadcastManager>().editDetails.runAsync((
        id: widget.broadcast.id,
        title: titleChanged ? title : null,
        description: descriptionChanged ? description : null,
        start: startChanged ? start : null,
        zone: startChanged ? _zone : null,
      ));
      if (!mounted) return;
      context.showSuccessToast('Broadcast updated');
      Navigator.of(context).pop();
    } on Object catch (error) {
      if (!mounted) return;
      setState(() => _saving = false);
      context.showErrorToast(errorMessage(error));
    }
  }
}
