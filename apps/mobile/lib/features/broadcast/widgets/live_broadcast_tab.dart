import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_ui/meno_ui.dart';

class LiveBroadcastTab extends WatchingStatefulWidget {
  const new({super.key});

  @override
  State<LiveBroadcastTab> createState() => _LiveBroadcastTabState();
}

class _LiveBroadcastTabState extends State<LiveBroadcastTab>
    with SingleTickerProviderStateMixin {
  late TabController tabController;

  @override
  void initState() {
    super.initState();
    tabController = TabController(length: 2, vsync: this);
  }

  @override
  void dispose() {
    tabController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final broadcast = watchValue<LiveSessionManager, Broadcast>(
      (manager) => manager.broadcast,
    );

    final time = watchValue<BroadcastTimerManager, String>(
      (manager) => manager.formattedTime,
    );

    final timeAgo = watchValue<BroadcastTimerManager, String>((m) => m.timeAgo);

    final isRunning = watchValue<BroadcastTimerManager, bool>(
      (manager) => manager.isRunning,
    );

    return Column(
      key: const ValueKey('LiveBroadcastTab'),
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
                mainAxisAlignment: .center,
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
        SizedBox(
          height: 40,
          child: Padding(
            padding: const .only(top: Insets.sm),
            child: TabBar.secondary(
              controller: tabController,
              tabs: const [
                Tab(text: 'Listening'),
                Tab(text: 'About'),
              ],
            ),
          ),
        ),
        Expanded(
          child: TabBarView(
            controller: tabController,
            children: const [
              BroadcastListeningTab(key: Key('LiveBroadcastParticipantsTab')),
              BroadcastAboutTab(key: Key('LiveBroadcastAboutTab')),
            ],
          ),
        ),
      ],
    );
  }
}
