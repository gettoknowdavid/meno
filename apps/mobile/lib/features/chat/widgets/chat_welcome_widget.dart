import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/broadcast/manager/active_broadcast_manager.dart';
import 'package:meno_ui/meno_ui.dart';

/// The note a person sees the first time they open the room's chat.
class ChatWelcomeWidget extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final chat = di<ActiveBroadcastManager>().session?.chat;
    if (chat == null) return const SizedBox.shrink();

    final isVisible = watch(chat.welcomeMessageVisible).value;
    if (!isVisible) return const SizedBox.shrink();

    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);
    return Container(
      height: 88,
      padding: const .all(Insets.lg),
      margin: const .symmetric(horizontal: Insets.lg, vertical: Insets.sm),
      decoration: BoxDecoration(
        color: colors.primaryContainer,
        borderRadius: .circular(Insets.md),
      ),
      child: Row(
        mainAxisSize: .min,
        crossAxisAlignment: .start,
        children: [
          Container(
            width: Insets.xxl,
            height: Insets.xxl,
            padding: const .symmetric(vertical: 10),
            clipBehavior: .antiAlias,
            decoration: BoxDecoration(
              color: colors.onPrimaryContainer,
              borderRadius: .circular(Insets.md),
            ),
            child: Assets.images.logoLight.svg(package: 'meno_ui'),
          ),
          Spaces.horizontalLarge,
          Expanded(
            child: Column(
              crossAxisAlignment: .start,
              children: [
                SizedBox(
                  height: Insets.xxl,
                  child: MText(
                    '''Welcome to the live chat! We encourage you to be as interactive as you can.''',
                    style: textTheme.microRegular,
                  ),
                ),
                Spaces.verticalSmall,
                SizedBox(
                  height: Insets.lg,
                  child: MTextButton(
                    label: 'OK, THANK YOU',
                    onPressed: chat.hideWelcomeNote,
                    style: TextButton.styleFrom(
                      textStyle: textTheme.microMedium,
                      padding: .zero,
                    ),
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
