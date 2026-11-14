import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/features.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:skeletonizer/skeletonizer.dart';

class ParticipantsGrid extends WatchingWidget {
  const new(
    this.participants, {
    super.key,
    this.padding,
    this.isSearching = false,
    this.isLoading = false,
  });

  final List<Participant?> participants;
  final EdgeInsetsGeometry? padding;
  final bool isSearching;
  final bool isLoading;

  @override
  Widget build(BuildContext context) {
    if (isSearching && participants.isEmpty) return const MEmptyState();

    final currentUserId = watchValue<UserManager, Id?>(
      (manager) => manager.currentUserId,
    );

    final effectiveParticipants = isLoading
        ? fakeParticipantList
        : participants;

    return Skeletonizer(
      enabled: isLoading,
      child: GridView.builder(
        padding: padding ?? const .symmetric(horizontal: 16),
        gridDelegate: const SliverGridDelegateWithFixedCrossAxisCount(
          crossAxisCount: 4,
          crossAxisSpacing: Insets.sm,
          mainAxisSpacing: Insets.lg,
          childAspectRatio: 80 / 88,
        ),
        itemCount: effectiveParticipants.length,
        itemBuilder: (context, index) {
          final participant = effectiveParticipants[index]!;
          final id = participant.id;
          return AbsorbPointer(
            absorbing: isLoading,
            child: _Item(
              key: ValueKey(id),
              participant: participant,
              isIAm: id == currentUserId,
            ),
          );
        },
      ),
    );
  }
}

class _Item extends StatelessWidget {
  const new({required this.participant, required this.isIAm, super.key});

  final Participant participant;
  final bool isIAm;

  @override
  Widget build(BuildContext context) {
    return ParticipantItemWidget(
      key: key,
      participant: participant,
      onTap: isIAm ? null : () => _onTap(context),
    );
  }

  void _onTap(BuildContext ctx) => ParticipantInfoModal.show(ctx, participant);
}
