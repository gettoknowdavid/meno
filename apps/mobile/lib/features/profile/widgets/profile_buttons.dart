import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/profile/model/_model.dart';
import 'package:meno_mobile/features/profile/widgets/profile_editor_modal.dart';
import 'package:meno_ui/meno_ui.dart';

/// Follows or unfollows the account whose profile is open.
///
/// The state it draws is the proxy's, not a local flag: following somebody on
/// their page moves the same [UserProfileProxy] a broadcast card behind it is
/// drawing, so the card cannot disagree with the button that changed it. The
/// tap is optimistic — the label flips before the request leaves — and the
/// command's undo stack is what puts it back when the request fails.
///
/// A failure is *not* reported here. The page this button sits on listens to
/// [`toggleFollow`]'s errors and shows them in the app's own toast; a button
/// that grew its own error surface would be a second voice for one failure.
class SubscribeButton extends WatchingWidget {
  /// Creates the button for [profile].
  const new({required this.profile, super.key});

  /// The profile whose follow state this button moves.
  final UserProfileProxy profile;

  @override
  Widget build(BuildContext context) {
    // Watching the proxy is what makes the optimistic flip visible: it notifies
    // the moment the override moves, and again when the API settles it.
    watch(profile);
    final isRunning = watch(profile.toggleFollow.isRunning).value;
    final isFollowing = profile.isFollowing;

    if (isFollowing) {
      return MSecondaryButton(
        label: 'Subscribed',
        onPressed: isRunning ? null : profile.toggleFollow.run,
        style: OutlinedButton.styleFrom(
          textStyle: MTextTheme.of(context).microMedium,
          shape: const RoundedRectangleBorder(borderRadius: Corners.sm),
        ),
      );
    }

    return MPrimaryButton(
      label: 'Subscribe',
      onPressed: isRunning ? null : profile.toggleFollow.run,
      style: ElevatedButton.styleFrom(
        textStyle: MTextTheme.of(context).microMedium,
        shape: const RoundedRectangleBorder(borderRadius: Corners.sm),
      ),
    );
  }
}

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
