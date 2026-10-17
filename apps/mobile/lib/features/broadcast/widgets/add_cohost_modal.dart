import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_mobile/features/profile/model/_model.dart';
import 'package:meno_ui/meno_ui.dart';

class AddCohostModal extends WatchingWidget {
  const new _() : super(key: null);

  static Future<dynamic> show(BuildContext context) {
    return showModalBottomSheet<dynamic>(
      context: context,
      builder: (context) => const AddCohostModal._(),
      isScrollControlled: true,
      useRootNavigator: true,
    );
  }

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);

    final manager = di<BroadcastEditorManager>();

    final cohostsIds = watchValue<BroadcastEditorManager, List<Id>>(
      (manager) => manager.cohosts,
    );

    return MModal(
      title: 'Add Co-host',
      builder: (context) => SingleChildScrollView(
        padding: MediaQuery.viewInsetsOf(context),
        child: Column(
          mainAxisSize: .min,
          crossAxisAlignment: .stretch,
          children: [
            const MTextFormField(
              label: 'Search',
              prefixIcon: MIcons.search,
              showLabel: false,
              hint: 'Search',
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
                  'Select not more than 1 co-host',
                  color: colors.onBackgroundVariant,
                ),
              ],
            ),
            Spaces.verticalLarge,
            Expanded(
              child: ListView.separated(
                primary: false,
                separatorBuilder: (context, index) => Spaces.verticalXLarge,
                itemCount: fakeProfiles.length,
                itemBuilder: (context, index) {
                  final profile = fakeProfiles[index];
                  return CohostListTile(
                    profile: profile,
                    isDisabled: cohostsIds.isNotEmpty,
                    onSelect: manager.toggleCohost,
                  );
                },
              ),
            ),
            Spaces.verticalXXLarge,
            MPrimaryButton(label: 'Done', onPressed: context.pop),
          ],
        ),
      ),
    );
  }
}

class CohostListTile extends StatelessWidget {
  const new({
    required this.profile,
    required this.onSelect,
    this.isDisabled = false,
    super.key,
  });

  final Profile profile;
  final void Function(Id) onSelect;
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
            const MAvatar(radius: 24),
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
                label: 'Add as Co-host',
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
