import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_ui/meno_ui.dart';

class BroadcastTabWidget extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final broadcast = watchValue<LiveSessionManager, Broadcast>(
      (manager) => manager.broadcast,
    );

    final time = watchValue<LiveSessionManager, String>(
      (manager) => manager.timer.formattedTime,
    );

    final timeAgo = watchValue<LiveSessionManager, String>(
      (manager) => manager.timer.timeAgo,
    );

    final isRunning = watchValue<LiveSessionManager, bool>(
      (manager) => manager.timer.isRunning,
    );

    return DefaultTabController(
      length: 2,
      child: Column(
        children: [
          Padding(
            padding: const .fromLTRB(16, 24, 16, 16),
            child: Column(
              crossAxisAlignment: .stretch,
              children: [
                BroadcastArtworkWidget(image: broadcast.image),
                Spaces.verticalSmall,
                MTimerDisplay(
                  formattedTime: time,
                  isRunning: isRunning,
                  timeAgo: timeAgo,
                ),
                Spaces.verticalSmall,
                BroadcastTitleWidget(title: broadcast.title.orEmpty),
                Spaces.verticalSmall,
                Row(
                  mainAxisAlignment: MainAxisAlignment.center,
                  children: [
                    BroadcastCreatorWidget(name: broadcast.hostName.orEmpty),
                    Spaces.horizontalSmall,
                    const BroadcastStatusWidget(),
                  ],
                ),
                Spaces.verticalXLarge,
                const BroadcastControlButtons(),
              ],
            ),
          ),
          const SizedBox(
            height: 40,
            child: Padding(
              padding: EdgeInsets.only(top: Insets.sm),
              child: TabBar.secondary(
                tabs: [
                  Tab(text: 'Listening'),
                  Tab(text: 'About'),
                ],
              ),
            ),
          ),
          const Expanded(
            child: TabBarView(
              children: [BroadcastListeningTab(), BroadcastAboutTab()],
            ),
          ),
        ],
      ),
    );
  }
}
