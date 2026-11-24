import 'package:flutter/foundation.dart' show ValueListenable;
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';

/// The card a floating window draws: the broadcast, not the room.
///
/// A Picture-in-Picture window is a status surface. It has no tabs, no chat and
/// no roster, and it takes no touches — the two window buttons are the only
/// actions. So this widget is the "listening card" reduced to a 16:9 frame:
/// cover, title, the live badge and clock, who the account is in the room, and
/// whether it is muted.
///
/// # Why listenables, not values
///
/// The window keeps drawing while the app is backgrounded (the foreground
/// service is what keeps the isolate alive), so the clock has to advance and a
/// reconnect has to appear without the app rebuilding anything. It reads the
/// manager's snapshot and the session's clock directly through their notifiers
/// rather than being handed a snapshot, which would freeze both.
///
/// # No design-system theme
///
/// The content is built above the app's `Navigator`, and its background is the
/// system window: colours and type are fixed here rather than taken from
/// `MColorScheme`, so a light-theme account still gets readable white text on
/// the cover's scrim.
class BroadcastMiniStage extends StatelessWidget {
  /// Creates the window's content.
  const new({
    required this.snapshot,
    required this.elapsed,
    required this.microphoneEnabled,
    super.key,
  });

  /// The room's one authoritative view: broadcast, phase, and what this
  /// account may do.
  final ValueListenable<LiveSnapshot> snapshot;

  /// The session's clock, as `HH:MM:SS`.
  final ValueListenable<String> elapsed;

  /// Whether this account's microphone is publishing.
  final ValueListenable<bool> microphoneEnabled;

  @override
  Widget build(BuildContext context) {
    return Material(
      color: Colors.black,
      child: ValueListenableBuilder<LiveSnapshot>(
        valueListenable: snapshot,
        builder: (context, live, _) {
          final broadcast = live.broadcast ?? Broadcast.empty;
          final cover = broadcast.image?.getUrl;

          return Stack(
            fit: StackFit.expand,
            children: [
              if (cover != null)
                Image.network(
                  cover,
                  fit: BoxFit.cover,
                  // A cover that will not load must not blank the window: the
                  // stage's job is the status, and the scrim alone is readable.
                  errorBuilder: (_, _, _) =>
                      const ColoredBox(color: Colors.black),
                ),
              const DecoratedBox(
                decoration: BoxDecoration(
                  gradient: LinearGradient(
                    begin: Alignment.topCenter,
                    end: Alignment.bottomCenter,
                    colors: [Colors.black45, Colors.black87],
                  ),
                ),
              ),
              Padding(
                padding: const EdgeInsets.all(12),
                child: Row(
                  children: [
                    _Cover(url: cover),
                    const SizedBox(width: 12),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        mainAxisAlignment: MainAxisAlignment.center,
                        children: [
                          Row(
                            children: [
                              const _LiveBadge(),
                              const SizedBox(width: 8),
                              ValueListenableBuilder<String>(
                                valueListenable: elapsed,
                                builder: (context, time, _) => Text(
                                  time,
                                  style: const TextStyle(
                                    color: Colors.white70,
                                    fontSize: 12,
                                  ),
                                ),
                              ),
                            ],
                          ),
                          const SizedBox(height: 4),
                          Text(
                            broadcast.title.orEmpty,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: const TextStyle(
                              color: Colors.white,
                              fontSize: 15,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                          const SizedBox(height: 2),
                          Text(
                            _status(live, broadcast),
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: const TextStyle(
                              color: Colors.white70,
                              fontSize: 12,
                            ),
                          ),
                        ],
                      ),
                    ),
                    if (live.canPublish)
                      ValueListenableBuilder<bool>(
                        valueListenable: microphoneEnabled,
                        builder: (context, enabled, _) =>
                            enabled ? const SizedBox.shrink() : const _Muted(),
                      ),
                  ],
                ),
              ),
            ],
          );
        },
      ),
    );
  }

  /// Who the account is in this room, in one line.
  static String _status(LiveSnapshot live, Broadcast broadcast) {
    if (live.phase == LivePhase.reconnecting ||
        live.phase == LivePhase.connecting) {
      return 'Reconnecting…';
    }
    if (live.canEndBroadcast) return "You're live";
    if (live.canPublish) return "You're live as a cohost";
    return 'Listening · ${broadcast.hostName.orEmpty}';
  }
}

/// The circular cover, or the host's initial while there is no image.
class _Cover extends StatelessWidget {
  const new({required this.url});

  final String? url;

  @override
  Widget build(BuildContext context) {
    return ClipOval(
      child: SizedBox(
        width: 48,
        height: 48,
        child: url == null
            ? const ColoredBox(color: Colors.white24)
            : Image.network(
                url!,
                fit: BoxFit.cover,
                errorBuilder: (_, _, _) =>
                    const ColoredBox(color: Colors.white24),
              ),
      ),
    );
  }
}

/// The red `LIVE` pill, the same vocabulary the room uses.
class _LiveBadge extends StatelessWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
      decoration: BoxDecoration(
        color: Colors.red,
        borderRadius: BorderRadius.circular(4),
      ),
      child: const Text(
        'LIVE',
        style: TextStyle(
          color: Colors.white,
          fontSize: 9,
          fontWeight: FontWeight.w700,
          letterSpacing: 0.6,
        ),
      ),
    );
  }
}

/// The muted state: what the room's red microphone says, in text.
///
/// Deliberately not an icon: the window's typography is fixed here, and a
/// glyph that depends on a font asset is one more thing that can fail to load
/// in a surface nobody can troubleshoot from inside the app.
class _Muted extends StatelessWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
      decoration: BoxDecoration(
        color: Colors.red,
        borderRadius: BorderRadius.circular(999),
      ),
      child: const Text(
        'Muted',
        style: TextStyle(
          color: Colors.white,
          fontSize: 11,
          fontWeight: FontWeight.w600,
        ),
      ),
    );
  }
}
