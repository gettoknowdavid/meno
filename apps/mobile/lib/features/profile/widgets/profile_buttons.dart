import 'package:flutter/material.dart';
import 'package:meno/features/profile/profile.dart';
import 'package:meno/features/profile/widgets/profile_editor_modal.dart';
import 'package:meno_design_system/meno_design_system.dart';
import 'package:skeletonizer/skeletonizer.dart';

class EditProfileButton extends StatelessWidget {
  const EditProfileButton({required this.profile, super.key});

  final Profile profile;

  @override
  Widget build(BuildContext context) {
    const shape = RoundedRectangleBorder(borderRadius: Corners.sm);
    final textStyle = MTextTheme.of(context).microMedium;
    return MPrimaryButton.icon(
      label: 'Edit profile',
      icon: const Icon(MIcons.edit_05),
      onPressed: () => ProfileEditorModal.show(context, profile),
      style: ElevatedButton.styleFrom(textStyle: textStyle, shape: shape),
    );
  }
}

class ShareProfileButton extends StatelessWidget {
  const ShareProfileButton({required this.profile, super.key});

  final Profile profile;

  @override
  Widget build(BuildContext context) {
    const shape = RoundedRectangleBorder(borderRadius: Corners.sm);
    final textStyle = MTextTheme.of(context).microMedium;
    return Skeleton.unite(
      child: MSecondaryButton.icon(
        label: 'Share profile',
        icon: const Icon(MIcons.share),
        onPressed: () {},
        style: OutlinedButton.styleFrom(textStyle: textStyle, shape: shape),
      ),
    );
  }
}
