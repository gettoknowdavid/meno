import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_ui/meno_ui.dart';

/// The controls a person in a live room is offered.
///
/// Two different facts decide them, and neither is the other: the microphone
/// is shown when the API let this account publish (a cohost, or the creator),
/// while ending the broadcast is the creator's alone (audit H08). A listener is
/// offered only the way out.
class BroadcastControlButtons extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final snapshot = watchValue<ActiveBroadcastManager, LiveSnapshot>(
      (manager) => manager.snapshot,
    );

    final busy = snapshot.phase == LivePhase.ending;

    return SizedBox(
      height: 40,
      child: Row(
        mainAxisAlignment: .center,
        children: [
          if (snapshot.canPublish) ...[
            const _MicrophoneButton(key: Key('BroadcastMicrophoneButton')),
            Spaces.horizontalSmall,
          ],
          if (snapshot.canEndBroadcast) ...[
            _ExitButton(
              key: const Key('BroadcastStopButton'),
              label: 'Stop broadcasting',
              isEnding: true,
              enabled: !busy,
            ),
          ] else ...[
            _ExitButton(
              key: const Key('LeaveBroadcastButton'),
              label: 'Leave broadcast',
              isEnding: false,
              enabled: !busy,
            ),
          ],
          Spaces.horizontalSmall,
          const BroadcastOptionsButton(),
        ],
      ),
    );
  }
}

class _MicrophoneButton extends WatchingStatefulWidget {
  const new({super.key});

  @override
  State<_MicrophoneButton> createState() => _MicrophoneButtonState();
}

class _MicrophoneButtonState extends State<_MicrophoneButton> {
  /// The last state this button asked for, until the session reports its own.
  bool? _enabled;

  @override
  Widget build(BuildContext context) {
    final session = di<ActiveBroadcastManager>().session;
    final enabled = _enabled ?? session?.isMicrophoneEnabled ?? false;

    return MMicrophoneButton(
      isMicrophoneEnabled: enabled,
      onTap: session == null
          ? null
          : () {
              setState(() => _enabled = !enabled);
              unawaited(session.setMicrophoneEnabled(!enabled));
            },
    );
  }
}

/// Ends the broadcast (creator) or leaves it (everyone else).
///
/// The dialog asks which of the two the person means, and the command decides
/// the endpoint from creator authority — never from this button's label.
class _ExitButton extends WatchingWidget {
  const new({
    required this.label,
    required this.isEnding,
    required this.enabled,
    super.key,
  });

  final String label;
  final bool isEnding;
  final bool enabled;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    final isRunning = watchValue<ActiveBroadcastManager, bool>(
      (manager) => manager.leaveOrEnd.isRunning,
    );

    final style = ElevatedButton.styleFrom(
      backgroundColor: colors.errorContainer.withValues(alpha: 0.3),
      foregroundColor: colors.error,
      iconColor: colors.errorContainer,
      fixedSize: const Size(159, 40),
      padding: const .symmetric(horizontal: Insets.lg, vertical: Insets.sm),
      textStyle: textTheme.captionMedium,
      shape: const RoundedRectangleBorder(borderRadius: Corners.circle),
    );

    Future<void> onPressed() async {
      final manager = di<ActiveBroadcastManager>();
      final confirmed = await BroadcastExitAlertDialog.show(context, isEnding);
      if (!(confirmed ?? false)) return;
      // The manager is resolved again after the dialog: a switch during it
      // takes the old one away with the account.
      if (!di.isRegistered<ActiveBroadcastManager>()) return;
      await manager.leaveOrEnd.runAsync();
    }

    if (isEnding) {
      return MPrimaryButton(
        label: label,
        onPressed: enabled ? onPressed : null,
        loading: isRunning,
        style: style,
      );
    }

    return MPrimaryButton.icon(
      label: label,
      icon: const Icon(MIcons.log_out),
      onPressed: enabled ? onPressed : null,
      loading: isRunning,
      style: style,
    );
  }
}
