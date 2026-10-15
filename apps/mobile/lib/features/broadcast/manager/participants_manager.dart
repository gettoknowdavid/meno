import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';
import 'package:meno_mobile/features/broadcast/services/_services.dart';

/// Who is in one live room, as the room's screens draw them.
///
/// Two sources feed it and both are needed: the API's participant page is the
/// state of the room at the moment the screen opened, and the socket's join/leave
/// events are every change after that. Neither alone is enough — a socket-only
/// manager would show an empty room to a client that joined late, and an
/// HTTP-only one would never notice a listener arriving.
///
/// Ordering is the manager's own rule rather than the API's: the room reads
/// host, then cohosts, then listeners, which is a presentation decision the API
/// has no reason to make. It is applied on insert so the list is never
/// re-sorted under a scrolled tile.
final class ParticipantsManager implements Disposable {
  new({
    required this._http,
    required this._socket,
    required this._broadcastId,
  }) {
    initialize = Command.createSyncNoParamNoResult(() {
      if (_isInitialized) return;

      _onParticipantJoined = _socket.onParticipantJoined.listen(_handleJoined);
      _onParticipantLeft = _socket.onParticipantLeft.listen(_handleLeft);

      searchQuery
          .debounce(const Duration(milliseconds: _searchDebounceMs))
          .listen(_handleSearchChange);

      _isInitialized = true;
    }, errorFilterFn: menoExceptionFilter)..pipeToCommand(_fetchParticipants);

    _fetchParticipants = Command.createAsyncNoParamNoResult(() async {
      final page = await _http.participants(_broadcastId);
      _handleInitialList(page.items);
    }, errorFilterFn: menoExceptionFilter);

    /// Shows [ _pageSize] more of the participants already loaded.
    loadMore = Command.createSyncNoParamNoResult(() {
      final current = displayedCount.value;
      if (current >= _sortedParticipants.length) return;

      displayedCount.value = (current + _pageSize).clamp(0, _maxWithoutViewAll);
    });

    /// Shows everything loaded, up to the ceiling a very large room allows.
    showAll = Command.createSyncNoParamNoResult(() {
      displayedCount.value = _sortedParticipants.length.clamp(
        0,
        _maxWithoutViewAll,
      );
    });

    resetPagination = Command.createSyncNoParamNoResult(() {
      displayedCount.value = _initialDisplayCount;
    });

    updateSearch = Command.createSyncNoResult((query) {
      searchQuery.value = query.trim();
    });

    clearSearch = Command.createSyncNoParamNoResult(() {
      searchQuery.value = '';
      isSearching.value = false;
    });
  }

  final BroadcastHttpService _http;
  final BroadcastSocketService _socket;

  /// The room this manager is about.
  ///
  /// An id rather than the whole [BroadcastSession]: the roster needs to know
  /// *which* room, and taking only that is what lets a room's scope build this
  /// manager without the token that entering the room needs.
  final String _broadcastId;

  static const int _initialDisplayCount = 50;
  static const int _pageSize = 50;
  static const int _maxWithoutViewAll = 200;
  static const int _searchDebounceMs = 300;

  StreamSubscription<BroadcastParticipantEvent>? _onParticipantJoined;
  StreamSubscription<BroadcastParticipantEvent>? _onParticipantLeft;

  bool _isInitialized = false;

  /// Every account currently in the room, by id — the O(1) half of `left`.
  final _participantsMap = <String, Participant>{};

  /// Ids that joined at any point in this broadcast's life. A set, because the
  /// all-time count must not be inflated by somebody reloading the screen.
  final Set<String> _allTimeIds = {};

  /// Current participants, host first.
  final _sortedParticipants = ListNotifier<Participant>(data: []);

  /// Live participants, as the API last stated the count.
  final liveCount = ValueNotifier<int>(0);

  /// Everybody who joined at any point (only ever goes up).
  final allTimeCount = ValueNotifier<int>(0);

  /// The last three unique joiners, for an avatar stack.
  final recentParticipants = ListNotifier<Participant>(data: []);

  final searchQuery = ValueNotifier<String?>(null);
  final displayedCount = ValueNotifier<int>(_initialDisplayCount);
  final isSearching = ValueNotifier<bool>(false);

  /// Subscribes to the socket, then reads the room once.
  ///
  ///
  /// The order matters: a read taken before the subscriptions exist would miss
  /// every join between the response and the first listener.
  late final Command<void, void> initialize;

  late final Command<void, void> _fetchParticipants;

  /// Shows [ _pageSize] more of the participants already loaded.
  late final Command<void, void> loadMore;

  /// Shows everything loaded, up to the ceiling a very large room allows.
  late final Command<void, void> showAll;

  late final Command<void, void> resetPagination;

  late final Command<String, void> updateSearch;

  late final Command<void, void> clearSearch;

  late final ValueListenable<bool> isLoading = _fetchParticipants.isRunning;

  /// How many accounts are in the room right now.
  late final ValueListenable<int> totalCount =
      (_sortedParticipants as ValueListenable<List<Participant>>).map(
        (list) => list.length,
      );

  /// The host, when the room lists one.
  late final ValueListenable<Participant?> host =
      (_sortedParticipants as ValueListenable<List<Participant>>).map(
        (list) => list.where((p) => p.isHost).firstOrNull,
      );

  /// Every cohost in the room.
  late final ValueListenable<List<Participant>> cohosts =
      (_sortedParticipants as ValueListenable<List<Participant>>).map(
        (list) => list.where((p) => p.isCohost).toList(),
      );

  /// Every listener in the room.
  late final ValueListenable<List<Participant>> listeners =
      (_sortedParticipants as ValueListenable<List<Participant>>).map(
        (list) => list.where((p) => p.isListener).toList(),
      );

  /// The participants a list should draw: filtered, then paginated.
  ///
  /// Debounced because it is driven by a text field, and re-filtering a room of
  /// 200 on every keystroke is work nobody sees.
  late final ValueListenable<List<Participant>> displayedParticipants =
      searchQuery
          .combineLatest3(_sortedParticipants, displayedCount, (
            query,
            participants,
            count,
          ) {
            final trimmed = query?.trim().toLowerCase() ?? '';
            final filtered = trimmed.isEmpty
                ? participants
                : participants
                      .where((p) => p.fullName.toLowerCase().contains(trimmed))
                      .toList();

            return filtered.sublist(0, count.clamp(0, filtered.length));
          })
          .debounce(const Duration(milliseconds: _searchDebounceMs));

  /// Whether there are more loaded participants below the fold.
  late final ValueListenable<bool> hasMore = displayedCount.combineLatest(
    totalCount,
    (displayed, total) => displayed < total && displayed < _maxWithoutViewAll,
  );

  /// Whether the list is already showing everything loaded.
  late final ValueListenable<bool> isShowingAll = displayedCount.combineLatest(
    totalCount,
    (displayed, total) => displayed >= total,
  );

  void _handleJoined(BroadcastParticipantEvent event) {
    liveCount.value = event.count;
    final participant = event.participant;
    if (participant == null || participant.id.isEmpty) return;

    if (_participantsMap.containsKey(participant.id)) return;
    _participantsMap[participant.id] = participant;
    _insertIntoSortedList(participant);
    _rememberJoin(participant);
  }

  void _handleLeft(BroadcastParticipantEvent event) {
    liveCount.value = event.count;
    final participant = event.participant;
    if (participant == null) return;

    if (_participantsMap.remove(participant.id) == null) return;
    _sortedParticipants.removeWhere((item) => item.id == participant.id);
  }

  void _handleInitialList(List<Participant> participants) {
    _sortedParticipants.startTransAction();
    _sortedParticipants.clear();
    _participantsMap.clear();

    for (final participant in participants) {
      if (participant.id.isEmpty) continue;
      _participantsMap[participant.id] = participant;
      _insertIntoSortedList(participant);
      _rememberJoin(participant);
    }
    _sortedParticipants.endTransAction();
    liveCount.value = _sortedParticipants.length;
  }

  /// Counts a unique joiner, and keeps them in the recent-joiners stack.
  void _rememberJoin(Participant participant) {
    if (!_allTimeIds.add(participant.id)) return;

    allTimeCount.value++;

    recentParticipants.add(participant);
    if (recentParticipants.length > 3) recentParticipants.removeAt(0);
  }

  /// Inserts [participant] after every higher-ranked account.
  ///
  /// Inserted rather than appended-and-sorted so a tile already on screen keeps
  /// its position: a listener arriving must not shift the host.
  void _insertIntoSortedList(Participant participant) {
    final priority = _rolePriority(participant.role);
    var index = 0;
    for (; index < _sortedParticipants.length; index++) {
      if (_rolePriority(_sortedParticipants[index].role) > priority) break;
    }
    _sortedParticipants.insert(index, participant);
  }

  /// Lower sorts first.
  static int _rolePriority(ParticipantRole role) => switch (role) {
    ParticipantRole.host => 0,
    ParticipantRole.cohost => 1,
    ParticipantRole.listener => 2,
    ParticipantRole.unknown => 3,
  };

  void _handleSearchChange(String? query, ListenableSubscription _) {
    final isQueryEmpty = query == null || query.trim().isEmpty;
    isSearching.value = !isQueryEmpty;

    // A search reaches past the first page: matching only what is loaded would
    // answer "nobody called that" for someone who is in the room.
    if (!isQueryEmpty && displayedCount.value < _maxWithoutViewAll) {
      displayedCount.value = _maxWithoutViewAll;
    }
  }

  @override
  FutureOr<dynamic> onDispose() async {
    await _onParticipantJoined?.cancel();
    await _onParticipantLeft?.cancel();

    _onParticipantJoined = null;
    _onParticipantLeft = null;

    _sortedParticipants.dispose();
    recentParticipants.dispose();
    searchQuery.dispose();
    isSearching.dispose();
    displayedCount.dispose();
    liveCount.dispose();
    allTimeCount.dispose();

    _fetchParticipants.dispose();
    initialize.dispose();
    loadMore.dispose();
    showAll.dispose();
    resetPagination.dispose();
    updateSearch.dispose();
    clearSearch.dispose();

    _isInitialized = false;
  }
}
