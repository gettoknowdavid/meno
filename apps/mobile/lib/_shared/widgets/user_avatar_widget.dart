import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/value_objects/image_value_objects.dart';
import 'package:meno_mobile/_shared/model/entities.dart';
import 'package:meno_mobile/features/features.dart';
import 'package:meno_ui/meno_ui.dart';

class UserAvatarWidget extends WatchingWidget {
  const new({this.onTap, super.key});
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final user = watchValue<UserManager, User>(
      (manager) => manager.currentUser,
    );

    return MAvatar(
      radius: Insets.lg,
      url: user.image?.getUrl,
      onTap: onTap,
      hasBorder: false,
    );
  }
}
