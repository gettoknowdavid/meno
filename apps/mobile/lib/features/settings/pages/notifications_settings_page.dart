import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/settings/settings.dart';
import 'package:meno_ui/meno_ui.dart';

class NotificationsSettingsPage extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    final manager = di<SettingsManager>();
    final mainSettings = watchValue<SettingsManager, UserSettings>(
      (manager) => manager.settings,
    );
    final preferences = mainSettings.notificationPreferences;

    return MScaffold(
      appBar: MAppBar.secondary(title: 'Notifications', centerTitle: true),
      body: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Spaces.verticalLarge,
            MText(
              'Take control of how we keep you updated.',
              style: textTheme.captionMedium,
              color: colors.inActive,
            ),
            Spaces.verticalLarge,
            MSettingsSection(
              title: 'Global',
              children: [
                MSettingsListTile(
                  title: 'App Notifications',
                  trailing: Switch(
                    value: mainSettings.appNotifications,
                    onChanged: manager.setAppNotifications.run,
                  ),
                ),
                MSettingsListTile(
                  title: 'Push Notifications',
                  trailing: Switch(
                    value: mainSettings.pushNotifications,
                    onChanged: manager.setPushNotifications.run,
                  ),
                ),
                MSettingsListTile(
                  title: 'Email Notifications',
                  showDivider: false,
                  trailing: Switch(
                    value: mainSettings.emailNotifications,
                    onChanged: manager.setEmailNotifications.run,
                  ),
                ),
              ],
            ),
            Spaces.verticalXXLarge,
            MSettingsSection(
              title: 'Broadcasts',
              children: [
                MSettingsListTile(
                  title: 'Live broadcasts',
                  trailing: Switch(
                    value: preferences.liveBroadcastStarted,
                    onChanged: manager.setLiveBroadcastNotifications.run,
                  ),
                ),
                MSettingsListTile(
                  title: 'New subscribers',
                  trailing: Switch(
                    value: preferences.userSubscribed,
                    onChanged: manager.setSubscribersNotifications.run,
                  ),
                ),
                MSettingsListTile(
                  title: 'Added as co-host',
                  trailing: Switch(
                    value: preferences.addedAsCoHost,
                    onChanged: manager.setAddedAsCohostNotifications.run,
                  ),
                ),
                MSettingsListTile(
                  title: 'Scheduled Broadcasts',
                  showDivider: false,
                  trailing: Switch(
                    value: preferences.scheduledBroadcast,
                    onChanged: manager.setScheduledBroadcastNotifications.run,
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
