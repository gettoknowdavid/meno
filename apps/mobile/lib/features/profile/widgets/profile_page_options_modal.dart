import 'package:flutter/material.dart';
import 'package:meno_design_system/meno_design_system.dart';

class ProfilePageOptionsModal extends StatelessWidget {
  const ProfilePageOptionsModal._() : super(key: null);

  static Future<void> show(BuildContext context) {
    return showModalBottomSheet<void>(
      context: context,
      useRootNavigator: true,
      builder: (context) => const ProfilePageOptionsModal._(),
    );
  }

  @override
  Widget build(BuildContext context) {
    return MModal(
      builder: (context) => Column(
        mainAxisSize: .min,
        children: [
          MModalListTile(
            leading: const Icon(MIcons.user),
            title: 'Go to profile',
            onTap: () {},
          ),
          Spaces.verticalLarge,
          MModalListTile(
            leading: const Icon(MIcons.bell_ringing_04),
            title: 'Turn on notifications',
            onTap: () {},
          ),
          Spaces.verticalLarge,
        ],
      ),
    );
  }
}
