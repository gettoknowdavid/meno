import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/profile/model/_model.dart';
import 'package:meno_mobile/features/profile/widgets/profile_editor_modal.dart';
import 'package:meno_ui/meno_ui.dart';

/// Opens the editor for the signed-in account's profile.
class EditProfileButton extends StatelessWidget {
  /// Creates the button for [profile].
  const new({required this.profile, super.key});

  /// The profile being edited; the editor starts from it.
  final MyProfile profile;

  @override
  Widget build(BuildContext context) {
    return MPrimaryButton.icon(
      label: 'Edit profile',
      icon: const Icon(MIcons.edit_05),
      onPressed: () => ProfileEditorModal.show(context),
      style: ElevatedButton.styleFrom(
        textStyle: MTextTheme.of(context).microMedium,
        shape: const RoundedRectangleBorder(borderRadius: Corners.sm),
      ),
    );
  }
}

/// Shares a profile as a link.
///
/// Not wired yet: sharing means the platform share sheet, and which package
/// provides it — and whether the link is the web deep link or an app
/// invite — is a decision for the feature that owns sharing. The button keeps
/// its place in the layout until then.
class ShareProfileButton extends StatelessWidget {
  /// Creates the button for [profile].
  const new({required this.profile, super.key});

  /// The profile the link would point at.
  final Profile profile;

  @override
  Widget build(BuildContext context) {
    return MSecondaryButton.icon(
      label: 'Share profile',
      icon: const Icon(MIcons.share),
      onPressed: () {},
      style: OutlinedButton.styleFrom(
        textStyle: MTextTheme.of(context).microMedium,
        shape: const RoundedRectangleBorder(borderRadius: Corners.sm),
      ),
    );
  }
}
