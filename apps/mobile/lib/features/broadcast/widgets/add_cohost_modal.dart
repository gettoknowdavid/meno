import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_mobile/features/profile/model/_model.dart';
import 'package:meno_ui/meno_ui.dart';

/// The account search that adds a co-host.
///
/// Two callers, one sheet. The editor has no broadcast yet, so picking an
/// account puts it in the draft and the API learns about it at creation. A
/// broadcast that exists gets the handshake instead: picking an account invites
/// it, and nothing is added until the invitee says yes.
class AddCohostModal extends WatchingWidget {
  const new _({this.broadcastId});

  /// The broadcast to invite into, when there is one.
  ///
  /// Null in the editor: a draft's co-hosts are part of the creation payload,
  /// not invitations to answer.
  final Id? broadcastId;

  /// Opens the sheet, either to pick a draft's co-hosts or to invite into
  /// [broadcastId].
  static Future<dynamic> show(BuildContext context, {Id? broadcastId}) {
    return showModalBottomSheet<dynamic>(
      context: context,
      builder: (context) => AddCohostModal._(broadcastId: broadcastId),
      isScrollControlled: true,
      useRootNavigator: true,
    );
  }

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final search = di<CohostSearchManager>();

    // A fresh sheet starts from an empty search rather than from the last one,
    // which would otherwise still be there when the editor is reopened.
    callOnce((_) => search.clear());

    final editor = di<BroadcastEditorManager>();
    final cohostsIds = watchValue<BroadcastEditorManager, List<Id>>(
      (manager) => manager.cohosts,
    );
    final results = watchValue<CohostSearchManager, List<Profile>>(
      (manager) => manager.results,
    );
    final term = watchValue<CohostSearchManager, String>(
      (manager) => manager.query,
    );
    final isSearching = watchValue<CohostSearchManager, bool>(
      (manager) => manager.search.isRunning,
    );

    final isInviting = broadcastId != null;

    return MModal(
      title: isInviting ? 'Invite Co-host' : 'Add Co-host',
      builder: (context) => SingleChildScrollView(
        padding: MediaQuery.viewInsetsOf(context),
        child: Column(
          mainAxisSize: .min,
          crossAxisAlignment: .stretch,
          children: [
            MTextFormField(
              label: 'Search',
              prefixIcon: MIcons.search,
              showLabel: false,
              hint: 'Search',
              onChanged: search.onQueryChanged,
            ),
            Spaces.verticalLarge,
            Row(
              children: [
                Icon(
                  MIcons.info_circle,
                  size: 16,
                  color: colors.onBackgroundVariant,
                ),
                Spaces.horizontalMicro,
                MText(
                  isInviting
                      ? 'The invitation waits for their answer'
                      : 'Select up to '
                            '${BroadcastEditorManager.maxCohosts} co-hosts',
                  color: colors.onBackgroundVariant,
                ),
              ],
            ),
            Spaces.verticalLarge,
            if (results.isEmpty && !isSearching)
              SizedBox(
                height: 120,
                child: Center(
                  child: MText(
                    term.trim().isEmpty
                        ? 'Search for an account'
                        : 'No accounts found',
                    color: colors.onBackgroundVariant,
                  ),
                ),
              )
            else
              ListView.separated(
                primary: false,
                shrinkWrap: true,
                separatorBuilder: (context, index) => Spaces.verticalXLarge,
                itemCount: results.length,
                itemBuilder: (context, index) {
                  final profile = results[index];
                  final isPicked = cohostsIds.contains(profile.id);
                  return CohostListTile(
                    profile: profile,
                    isDisabled:
                        !isInviting &&
                        (isPicked ||
                            cohostsIds.length >=
                                BroadcastEditorManager.maxCohosts),
                    label: isPicked
                        ? 'Remove'
                        : isInviting
                        ? 'Invite'
                        : 'Add as Co-host',
                    onSelect: (id) async {
                      if (!isInviting) {
                        editor.toggleCohost(id, profile: profile);
                        return;
                      }
                      final invitation = await di<CohostInvitationsManager>()
                          .invite
                          .runAsync((broadcastId: broadcastId!, userId: id));
                      if (invitation == null || !context.mounted) return;
                      context.showSuccessToast('Invitation sent');
                    },
                  );
                },
              ),
            Spaces.verticalXXLarge,
            MPrimaryButton(label: 'Done', onPressed: context.pop),
          ],
        ),
      ),
    );
  }
}

/// One account in the sheet, with the one action the sheet has for it.
class CohostListTile extends StatelessWidget {
  const new({
    required this.profile,
    required this.onSelect,
    required this.label,
    this.isDisabled = false,
    super.key,
  });

  final Profile profile;
  final void Function(Id) onSelect;
  final String label;
  final bool isDisabled;

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);

    return Opacity(
      opacity: isDisabled ? 0.38 : 1.0,
      child: AbsorbPointer(
        absorbing: isDisabled,
        child: Row(
          children: [
            MAvatar(radius: 24, url: profile.avatarUrl?.getUrl),
            Spaces.horizontalSmall,
            Expanded(
              child: Column(
                crossAxisAlignment: .start,
                mainAxisAlignment: .center,
                children: [
                  MText(
                    profile.fullName.orEmpty,
                    style: textTheme.captionMedium,
                    overflow: .ellipsis,
                    maxLines: 1,
                  ),
                  const SizedBox(height: 3),
                  MText(
                    '${profile.stats.followers} followers',
                    style: textTheme.microRegular,
                  ),
                ],
              ),
            ),
            Spaces.horizontalLarge,
            SizedBox(
              height: 32,
              child: MSecondaryButton(
                label: label,
                style: OutlinedButton.styleFrom(
                  textStyle: textTheme.microMedium,
                  padding: const .symmetric(horizontal: Insets.lg),
                  shape: const RoundedRectangleBorder(borderRadius: Corners.sm),
                ),
                onPressed: () => onSelect(profile.id),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
