import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_mobile/features/broadcast/widgets/_widgets.dart';
import 'package:meno_ui/meno_ui.dart';

class BroadcastControlButtons extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final isHost = watchValue<LiveSessionManager, bool>(
      (manager) => manager.isHost,
    );

    return SizedBox(
      height: 40,
      child: Row(
        mainAxisAlignment: .center,
        children: [
          if (isHost) ...[
            const _MicrophoneButton(key: Key('BroadcastMicrophoneButton')),
            Spaces.horizontalSmall,
            const _StopBroadcastingButton(key: Key('BroadcastStopButton')),
          ] else ...[
            const _LeaveBroadcastButton(key: Key('LeaveBroadcastButton')),
          ],
          Spaces.horizontalSmall,
          const BroadcastOptionsButton(),
        ],
      ),
    );
  }
}

class _MicrophoneButton extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final enabled = watchValue<LiveSessionManager, bool>(
      (manager) => manager.isMicrophoneEnabled,
    );

    return MMicrophoneButton(
      isMicrophoneEnabled: enabled,
      onTap: () => di<LiveSessionManager>().toggleMicrophone.run(!enabled),
    );
  }
}

class _StopBroadcastingButton extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    final isEnding = watchValue<LiveSessionManager, bool>(
      (manager) => manager.endSession.isRunning,
    );

    return MPrimaryButton(
      label: 'Stop broadcasting',
      onPressed: () async {
        final result = await BroadcastExitAlertDialog.show(context, true);
        if (result ?? false) di<LiveSessionManager>().endSession.run();
      },
      loading: isEnding,
      style: ElevatedButton.styleFrom(
        backgroundColor: colors.errorContainer.withValues(alpha: 0.3),
        foregroundColor: colors.error,
        fixedSize: const Size(159, 40),
        padding: const .symmetric(horizontal: Insets.lg, vertical: Insets.sm),
        textStyle: textTheme.captionMedium,
        shape: const RoundedRectangleBorder(borderRadius: Corners.circle),
      ),
    );
  }
}

class _LeaveBroadcastButton extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    final isEnding = watchValue<LiveSessionManager, bool>(
      (manager) => manager.endSession.isRunning,
    );

    return MPrimaryButton.icon(
      label: 'Leave broadcast',
      icon: const Icon(MIcons.log_out),
      onPressed: () async {
        final result = await BroadcastExitAlertDialog.show(context, true);
        if (result ?? false) di<LiveSessionManager>().endSession.run();
      },
      loading: isEnding,
      style: ElevatedButton.styleFrom(
        backgroundColor: colors.errorContainer.withValues(alpha: 0.3),
        foregroundColor: colors.error,
        iconColor: colors.errorContainer,
        fixedSize: const Size(159, 40),
        padding: const .symmetric(horizontal: Insets.lg, vertical: Insets.sm),
        textStyle: textTheme.captionMedium,
        shape: const RoundedRectangleBorder(borderRadius: Corners.circle),
      ),
    );
  }
}
