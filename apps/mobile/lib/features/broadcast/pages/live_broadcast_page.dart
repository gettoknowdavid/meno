import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_ui/meno_ui.dart';

/// The live room's own screen.
///
/// Everything it draws comes from the manager's snapshot — the broadcast, the
/// account's standing in it and what that standing allows — and the clock comes
/// from the open session, which exists exactly while there is a room. There is
/// one implementation of this screen; the duplicate tab that resolved a timer
/// nobody had registered is gone (audit L05).
class LiveBroadcastPage extends WatchingStatefulWidget {
  const new({super.key});

  @override
  State<LiveBroadcastPage> createState() => _LiveBroadcastPageState();
}

class _LiveBroadcastPageState extends State<LiveBroadcastPage>
    with SingleTickerProviderStateMixin {
  late final TabController _tabs;

  @override
  void initState() {
    super.initState();
    _tabs = TabController(length: 2, vsync: this);
  }

  @override
  void dispose() {
    _tabs.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final manager = di<ActiveBroadcastManager>();

    final snapshot = watchValue<ActiveBroadcastManager, LiveSnapshot>(
      (manager) => manager.snapshot,
    );

    final broadcast = snapshot.broadcast ?? Broadcast.empty;
    final timer = manager.session?.timer;

    // The clock is the session's, so it is watched only while one is open.
    final time = timer == null ? '00:00:00' : watch(timer.formattedTime).value;
    final timeAgo = timer == null ? '' : watch(timer.timeAgo).value;
    final isRunning = timer != null && watch(timer.isRunning).value;

    return Column(
      key: const ValueKey('LiveBroadcastPage'),
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
              controller: _tabs,
              tabs: const [
                Tab(text: 'Listening'),
                Tab(text: 'About'),
              ],
            ),
          ),
        ),
        Expanded(
          child: TabBarView(
            controller: _tabs,
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
