import 'package:flutter/material.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/features/broadcast/manager/participants_manager.dart';
import 'package:meno_mobile/features/broadcast/model/entities.dart';
import 'package:meno_mobile/features/broadcast/widgets/participants_grid.dart';
import 'package:meno_ui/meno_ui.dart';

class ParticipantsModal extends WatchingWidget {
  const new _() : super(key: null);

  static Future<dynamic> show(BuildContext context) {
    final size = MediaQuery.sizeOf(context);
    return showModalBottomSheet<dynamic>(
      context: context,
      builder: (context) => const ParticipantsModal._(),
      isScrollControlled: true,
      useRootNavigator: true,
      constraints: BoxConstraints(maxHeight: size.height * 0.9),
    );
  }

  @override
  Widget build(BuildContext context) {
    final manager = di<ParticipantsManager>();

    final list = watchValue<ParticipantsManager, List<Participant>>(
      (m) => m.displayedParticipants,
    );

    final totalCount = watchValue<ParticipantsManager, int>(
      (m) => m.totalCount,
    );

    final isSearching = watchValue<ParticipantsManager, bool>(
      (m) => m.isSearching,
    );

    final isLoading = watchValue<ParticipantsManager, bool>((m) => m.isLoading);

    final query = watchValue<ParticipantsManager, String?>(
      (m) => m.searchQuery,
    );

    return PopScope(
      onPopInvokedWithResult: (didPop, result) {
        if (didPop) manager.clearSearch.run();
      },
      child: MModal(
        title: 'Listening ($totalCount)',
        builder: (context) => Column(
          mainAxisSize: .min,
          children: [
            MTextFormField(
              label: 'Search',
              prefixIcon: MIcons.search,
              showLabel: false,
              hint: 'Search',
              initialValue: query,
              onChanged: manager.updateSearch.run,
            ),
            Spaces.verticalLarge,
            Expanded(
              child: ParticipantsGrid(
                list,
                isSearching: isSearching,
                isLoading: isLoading,
                padding: .zero,
              ),
            ),
          ],
        ),
      ),
    );
  }
}
