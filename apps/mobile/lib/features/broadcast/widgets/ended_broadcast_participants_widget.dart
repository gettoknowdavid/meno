import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/model/entities.dart';
import 'package:meno_ui/meno_ui.dart';

class EndedBroadcastParticipantsWidget extends StatelessWidget {
  const new({
    required this.allTimeCount,
    required this.recentParticipants,
    super.key,
  });

  final int allTimeCount;
  final List<Participant> recentParticipants;

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);

    // Handle empty state
    if (allTimeCount == 0) {
      return Column(
        mainAxisSize: .min,
        children: [
          const SizedBox(height: 40),
          MText(
            'No listeners yet',
            style: textTheme.captionRegular,
            textAlign: TextAlign.center,
            color: MColorScheme.of(context).onBackgroundVariant,
          ),
        ],
      );
    }

    return Column(
      mainAxisSize: .min,
      children: [
        // Avatar stack
        MAvatarStack(
          participants: recentParticipants
              .map(
                (p) => ParticipantAvatar(
                  id: p.id.orEmpty,
                  avatarUrl: p.avatar?.getUrl,
                ),
              )
              .toList(),
          totalCount: allTimeCount,
        ),

        Spaces.verticalSmall,

        // Count text
        MText(
          _getPluralText(allTimeCount),
          style: textTheme.captionRegular,
          textAlign: TextAlign.center,
        ),
      ],
    );
  }

  String _getPluralText(int count) =>
      count == 1 ? '$count person tuned in!' : '$count people tuned in!';
}
