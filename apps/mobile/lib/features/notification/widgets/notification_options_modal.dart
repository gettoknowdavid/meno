import 'package:flutter_it/flutter_it.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart' hide Notification;
import 'package:meno_mobile/features/notification/model/_model.dart';
import 'package:meno_ui/meno_ui.dart';

class NotificationOptionsModal extends WatchingWidget {
  const new _(this.proxy) : super(key: null);
  final NotificationProxy proxy;

  static Future<dynamic> show(BuildContext context, NotificationProxy proxy) {
    return showModalBottomSheet<dynamic>(
      context: context,
      builder: (context) => NotificationOptionsModal._(proxy),
      isScrollControlled: true,
      useRootNavigator: true,
    );
  }

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final user = proxy.notification.content?.broadcastCreator ?? 'this user';
    return MModal(
      builder: (context) => Column(
        mainAxisSize: .min,
        spacing: 16,
        children: [
          MModalListTile(
            leading: const Icon(MIcons.bell_off_03),
            title: 'Turn off from $user',
            onTap: () {},
          ),
          MModalListTile(
            leading: Icon(MIcons.trash, color: colors.error),
            title: 'Delete',
            onTap: () {
              proxy.delete.run();
              context.pop();
            },
            titleColor: colors.error,
          ),
        ],
      ),
    );
  }
}
