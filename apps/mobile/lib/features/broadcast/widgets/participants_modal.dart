import 'package:flutter/material.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/features/broadcast/manager/active_broadcast_manager.dart';
import 'package:meno_mobile/features/broadcast/widgets/participants_grid.dart';
import 'package:meno_ui/meno_ui.dart';

class ParticipantsModal extends WatchingWidget {
  const new _() : super(key: null);

  /// Opens the sheet over [context].
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
    // The roster is the open session's, reached through the one registered live
    // owner (audit Step 31). A ParticipantsManager is never in the container,
    // so `di<ParticipantsManager>()` threw the moment this sheet was opened.
    final manager = di<ActiveBroadcastManager>().session?.participants;
    if (manager == null) {
      // Reachable only from the live shell, so nothing to list means the room
      // was closing as the sheet opened. An empty sheet, not a crash.
      return const SizedBox.shrink();
    }

    final list = watch(manager.displayedParticipants).value;
    final totalCount = watch(manager.totalCount).value;
    final isSearching = watch(manager.isSearching).value;
    final isLoading = watch(manager.isLoading).value;
    final query = watch(manager.searchQuery).value;

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
