import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// The overflow menu of somebody else's profile.
///
/// Both rows are the layout's, and both are still inert: the first is the page
/// the menu already sits on, and notifications belong to the notifications
/// feature. The menu keeps its place rather than being removed, so the action
/// that opens it does not disappear along with it.
class ProfilePageOptionsModal extends StatelessWidget {
  /// Prevents instantiation: [show] is the way in.
  const new _();

  /// Shows the menu over [context].
  static Future<void> show(BuildContext context) =>
      context.showModal<void>(const ProfilePageOptionsModal._());

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
