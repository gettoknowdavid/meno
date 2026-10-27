import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/widgets/user_avatar_widget.dart';
import 'package:meno_mobile/features/profile/manager/my_profile_manager.dart';
import 'package:meno_mobile/features/profile/model/_model.dart';

/// The signed-in account's avatar, read from its one presentation source.
///
/// Mounted only inside the ready, generation-keyed subtree (the gate), so it
/// resolves the account-scoped [MyProfileManager] directly and is disposed
/// with the account it belongs to. Nothing account-scoped is held by the root,
/// and this widget never touches a credential: name and image are plain data
/// handed down.
class CurrentUserAvatar extends WatchingWidget {
  const new({this.onTap, super.key});

  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final presentation = watchValue<MyProfileManager, MyProfile>(
      (manager) => manager.profile,
    );

    return UserAvatarWidget(
      name: presentation.fullName.orEmpty,
      imageUrl: presentation.avatarUrl?.getUrl,
      onTap: onTap,
    );
  }
}
