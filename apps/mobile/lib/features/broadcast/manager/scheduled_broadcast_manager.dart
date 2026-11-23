import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';
import 'package:meno_mobile/features/broadcast/services/_services.dart';

/// The account's one open scheduled broadcast, and the actions that resolve it.
///
/// # One at a time
///
/// The API refuses a second schedule while one is open. Open means not yet
/// cancelled and not yet ended after going live, so this manager never has to
/// choose between two. If the server ever answers with more than one, the
/// earliest start wins; the rest are left to the Scheduled tab, where the host
/// can cancel them.
///
/// # Why it watches the clock
///
/// The banner changes behaviour five minutes before the start without the home
/// screen polling: [isImminent] flips on a timer armed from the start time, so
/// the Hide button goes away exactly when the reminder window opens.
final class ScheduledBroadcastManager implements Disposable {
  new({
    required this._http,
    required this._accountId,
    required this._logger,
    this.socket,
  }) {
    refresh = .createAsyncNoParamNoResult(
      _load,
      errorFilterFn: menoExceptionFilter,
      debugName: 'broadcast.scheduled.refresh',
    );

    cancel = .createAsyncNoResult<Id>(
      (broadcastId) async {
        // Cancel is the API's soft delete: the broadcast was never on air, so
        // there is nothing to tear down — only a promise to withdraw.
        await _http.remove(broadcastId);
        await _load();
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'broadcast.scheduled.cancel',
    );

    reschedule = .createAsyncNoResult<ScheduleChange>(
      (change) async {
        await _http.update(
          change.id,
          startTime: change.start,
          timeZone: change.zone,
        );
        await _load();
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'broadcast.scheduled.reschedule',
    );

    editDetails = .createAsyncNoResult<ScheduleEdit>(
      (change) async {
        // Only what changed travels: the API refuses a start time in the past,
        // and a schedule that has missed its moment is exactly the row a host
        // may still want to correct the title of.
        await _http.update(
          change.id,
          title: change.title,
          description: change.description,
          startTime: change.start,
          timeZone: change.zone,
        );
        await _load();
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'broadcast.scheduled.edit',
    );

    // The socket is the other half of "what is scheduled right now": a schedule
    // made on another device, a cancellation, or the reminder frame itself. One
    // deferred refresh collapses a burst into a single read.
    final frames = socket;
    _handles = frames == null
        ? const []
        : [
            frames.onScheduledBroadcast.listen((_) => _refreshSoon()),
            frames.onBroadcastDeleted.listen((_) => _refreshSoon()),
            frames.onEndedBroadcast.listen((_) => _refreshSoon()),
            frames.onHomeInvalidated.listen((_) => _refreshSoon()),
          ];
  }

  final BroadcastHttpService _http;

  /// The frames that make a schedule stale, when this manager is wired to one.
  /// Null in tests that drive it through the API alone.
  final BroadcastSocketService? socket;

  final Id _accountId;
  final MenoLogger _logger;

  /// The account's next open schedule, or null when there is none.
  final ValueNotifier<Broadcast?> next = ValueNotifier(null);

  /// Whether the host asked to hide the banner.
  ///
  /// Only honoured until the reminder window opens: from five minutes before
  /// the start the banner is a prompt, not a notice, and a dismissed prompt
  /// is a broadcast nobody went live on.
  final ValueNotifier<bool> hidden = ValueNotifier(false);

  /// Whether the start is five minutes away or closer.
  final ValueNotifier<bool> isImminent = ValueNotifier(false);

  /// Whether the start time has arrived.
  final ValueNotifier<bool> isDue = ValueNotifier(false);

  /// Whether the start time passed and the grace period is over.
  ///
  /// A host still live on another broadcast cannot start this one, and the app
  /// must not start it for them. It waits: the banner keeps prompting, and only
  /// once [graceWindow] has passed does it say the moment was missed and offer
  /// the reschedule/cancel sheet. Nothing here cancels anything on its own —
  /// the decision stays with the host.
  final ValueNotifier<bool> isOverdue = ValueNotifier(false);

  /// How long a start may be late before the banner calls it missed.
  ///
  /// Ten minutes: long enough for a host to finish the sentence they are in on
  /// another broadcast, short enough that a schedule does not sit past its
  /// moment looking like it is about to begin.
  static const Duration graceWindow = Duration(minutes: 10);

  /// How long after its start a schedule keeps its place in the home banner.
  ///
  /// Thirty minutes: the prompt has had its chance by then, and a card that
  /// stays for a broadcast nobody started is a card that asks the same
  /// question forever. It is not cancelled — the Scheduled tab still lists it —
  /// so the host can go live late, reschedule or cancel from there.
  static const Duration bannerWindow = Duration(minutes: 30);

  /// Re-reads the account's open schedule.
  late final Command<void, void> refresh;

  /// Cancels it, freeing the slot for another schedule.
  late final Command<Id, void> cancel;

  /// Moves it to a new time, keeping it the one open schedule.
  late final Command<ScheduleChange, void> reschedule;

  /// Rewrites the details the host can see: title, description, start time.
  late final Command<ScheduleEdit, void> editDetails;

  late final List<StreamSubscription<void>> _handles;

  /// Every open schedule the last read returned, earliest first.
  ///
  /// The whole page rather than only the chosen one, so the banner can move on
  /// to the next promise the moment this one's window closes, without another
  /// request.
  List<Broadcast> _open = const [];

  Timer? _imminentTimer;
  Timer? _dueTimer;
  Timer? _overdueTimer;
  Timer? _expiryTimer;
  Timer? _refreshDebounce;

  bool _disposed = false;

  /// Whether the banner should be on screen.
  bool get isVisible {
    final broadcast = next.value;
    if (broadcast == null) return false;
    // Past the reminder window the banner cannot be hidden any more.
    return !hidden.value || isImminent.value;
  }

  /// Whether the Hide button belongs on the banner.
  bool get canHide => next.value != null && !isImminent.value;

  /// Hides the banner until the reminder window opens.
  void hide() {
    if (!canHide) return;
    hidden.value = true;
  }

  /// How long until the start, for the banner's subline.
  Duration? get untilStart {
    final start = next.value?.startTime;
    if (start == null) return null;
    final difference = start.toLocal().difference(DateTime.now());
    return difference.isNegative ? Duration.zero : difference;
  }

  Future<void> _load() async {
    final page = await _http.list(
      BroadcastQuery.scheduled(
        creatorId: _accountId.getOrCrash,
        limit: 10,
      ),
    );

    _open = [
      for (final summary in page.items)
        if (summary.startTime != null && !summary.hasEnded)
          summary.toBroadcast(),
    ]..sort((a, b) => a.startTime!.compareTo(b.startTime!));

    _applyNext();
  }

  /// Chooses the banner's schedule from [_open].
  ///
  /// A start whose [bannerWindow] has closed is past being useful on the home
  /// screen, but it is still an open promise: it is left to the Scheduled tab,
  /// and the banner moves on to the next row (or nothing) instead.
  void _applyNext() {
    final now = DateTime.now();
    final visible = [
      for (final broadcast in _open)
        if (broadcast.startTime!.toLocal().add(bannerWindow).isAfter(now))
          broadcast,
    ];

    final value = visible.isEmpty ? null : visible.first;
    final previousId = next.value?.id;

    next.value = value;

    // A different schedule is a new promise, so it gets its banner back. The
    // same one keeps the host's choice to hide it.
    if (value == null || value.id != previousId) hidden.value = false;

    _armTimers();
  }

  /// Arms the two moments the banner's behaviour changes at.
  void _armTimers() {
    _imminentTimer?.cancel();
    _imminentTimer = null;
    _dueTimer?.cancel();
    _dueTimer = null;
    _overdueTimer?.cancel();
    _overdueTimer = null;
    _expiryTimer?.cancel();
    _expiryTimer = null;

    final start = next.value?.startTime?.toLocal();
    if (start == null) {
      isImminent.value = false;
      isDue.value = false;
      isOverdue.value = false;
      return;
    }

    final now = DateTime.now();
    final lead = start.subtract(const Duration(minutes: 5));

    final graceEnds = start.add(graceWindow);

    // The moment the banner stops being the banner's business. Recompute from
    // the page in hand instead of asking the API: nothing changed on the
    // server, only the clock.
    final expires = start.add(bannerWindow);
    if (expires.isAfter(now)) {
      _expiryTimer = Timer(expires.difference(now), () {
        if (_disposed) return;
        _applyNext();
      });
    }

    isImminent.value = !lead.isAfter(now);
    isDue.value = !start.isAfter(now);
    isOverdue.value = !graceEnds.isAfter(now);

    if (!isImminent.value) {
      _imminentTimer = Timer(lead.difference(now), () {
        if (_disposed) return;
        isImminent.value = true;
      });
    }
    if (!isDue.value) {
      _dueTimer = Timer(start.difference(now), () {
        if (_disposed) return;
        isDue.value = true;
      });
    }
    if (!isOverdue.value) {
      _overdueTimer = Timer(graceEnds.difference(now), () {
        if (_disposed) return;
        isOverdue.value = true;
      });
    }
  }

  void _refreshSoon() {
    if (_disposed) return;
    _refreshDebounce?.cancel();
    _refreshDebounce = Timer(const Duration(milliseconds: 300), () {
      if (_disposed) return;
      _logger.d(
        'Refreshing the scheduled broadcast',
        tag: MenoLogTag.network.label,
      );
      refresh.run();
    });
  }

  @override
  FutureOr<dynamic> onDispose() async {
    _disposed = true;
    _imminentTimer?.cancel();
    _dueTimer?.cancel();
    _overdueTimer?.cancel();
    _expiryTimer?.cancel();
    _refreshDebounce?.cancel();

    for (final handle in _handles) {
      await handle.cancel();
    }

    refresh.dispose();
    cancel.dispose();
    reschedule.dispose();
    editDetails.dispose();
    next.dispose();
    hidden.dispose();
    isImminent.dispose();
    isDue.dispose();
    isOverdue.dispose();
  }
}

/// A new time for an existing schedule.
typedef ScheduleChange = ({Id id, DateTime start, String? zone});

/// The details of a schedule a host can correct.
///
/// Null means "leave it alone": the API's patch is partial, and sending a
/// start time it did not ask for is how an already-missed schedule would be
/// refused for being in the past.
typedef ScheduleEdit = ({
  Id id,
  String? title,
  String? description,
  DateTime? start,
  String? zone,
});
