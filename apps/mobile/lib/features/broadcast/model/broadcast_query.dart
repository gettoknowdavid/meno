import 'package:equatable/equatable.dart';
import 'package:meno_mobile/features/broadcast/model/entities.dart';

/// The audience a broadcast list is built for.
///
/// The feed carries this because it decides two things the wire does not say:
/// how a live socket event is applied to the list, and what the screen calls
/// itself. It never travels as a request parameter — see [BroadcastQuery].
enum BroadcastsType {
  /// The broadcasts of the accounts the viewer follows.
  forYou('forYou', 'Live for you'),

  /// Everything on air now.
  nowLive('nowLive', 'Now live'),

  /// Everything that has ended, most recent first.
  recentlyLive('recentlyLive', 'Recently live'),

  /// The accounts the viewer follows, as a list.
  subscriptions('subscriptions', 'Subscriptions');

  new(this.wire, this.title);

  /// The spelling that travels in a URL, for the router and nothing else.
  final String wire;

  /// The heading the section draws.
  final String title;

  static BroadcastsType? fromWire(String? wire) {
    for (final type in BroadcastsType.values) {
      if (type.wire == wire) return type;
    }
    return null;
  }
}

/// The filters one broadcast list is built from.
///
/// # What the endpoint can carry
///
/// `GET /broadcasts` takes `creatorId`, `status`, `order`, `cursor` and `limit`
/// — that is the whole filter set (`BroadcastQuery` in
/// `apps/api/src/modules/broadcast/dto.rs`, and `CursorParams` in
/// `crates/core/src/pagination.rs`; both are `camelCase` on the wire).
///
/// # Why there is no sort key, and no page number
///
/// The list is paginated by a keyset cursor over `(created_at, id)`: the cursor
/// names the row the next page starts after, *in that order*. That is what
/// makes the two halves of the old app's query object impossible to keep:
///
/// * a **sort key** — `SortBy.startTime`, `SortBy.endTime`, the three-key sort
///   — only makes sense if the cursor encodes the column it sits on. Order a
///   `nowLive` feed by `startTime asc` while the cursor walks `created_at desc`
///   and the boundary points at the wrong row: pages then repeat rows or drop
///   them, which is the failure the endpoint was written to avoid.
/// * a **page number** — `PaginationParams(page:, size:)` — counts rows, and
///   this list reorders as broadcasts start and end. Page 3 of a list that
///   shifted under you is not the third page of anything.
/// * **time ranges** (`TimeRange`, the eight bounds) are narrowing filters, so
///   a cursor could carry them, but they were dropped for a simpler reason:
///   the sections they built are already said by `status`, which is indexed and
///   which the socket events agree with.
///
/// A filter that only *narrows* the rows stays compatible with a keyset cursor
/// and can be added without redesigning any of this — that is the shape of the
/// two gaps noted below.
///
/// # Sections
///
/// The sections the UI draws are `all`, `nowLive`, `recentlyLive`, `forYou` and
/// `subscriptions`; `section` maps a [BroadcastsType] to its query so a tab bar
/// can be built from the enum. Two of them are not fully backed by the API yet,
/// and say so on their factories rather than pretending:
///
/// * `forYou` sends `status=active` and takes every live broadcast, because the
///   endpoint has no "who I follow" filter. Through
///   [BroadcastsType.subscriptions] the same gap appears a second time.
/// * there is no keyword filter at all, so the search bars in the design have
///   nothing to send yet. `GET /profiles` shows what adding one looks like: a
///   `q` matched against a search vector, sitting alongside the same cursor —
///   narrowing, so cursor-safe.
class BroadcastQuery with Equatable {
  const new({
    this.type,
    this.status,
    this.creatorId,
    this.order = BroadcastOrder.newest,
    this.cursor,
    this.limit = defaultLimit,
  });

  /// Everything, newest first — the Discover list with no tab chosen.
  const new all({this.limit = defaultLimit})
    : type = null,
      status = null,
      creatorId = null,
      order = BroadcastOrder.newest,
      cursor = null;

  /// Everything on air now, newest first.
  ///
  /// `created_at desc` is a stand-in for "most recently started": a broadcast
  /// is created and put on air by the same action, so the two orders agree for
  /// the live rows the section draws.
  const new nowLive({this.limit = defaultLimit})
    : type = BroadcastsType.nowLive,
      status = BroadcastStatus.active,
      creatorId = null,
      order = BroadcastOrder.newest,
      cursor = null;

  /// Everything that has ended, newest first.
  ///
  /// `inactive` rather than `ended`, because `inactive` is what this API
  /// writes: ending a broadcast sets `status = inactive` and `end_time`, and
  /// `BroadcastStatus` in `apps/api/src/modules/broadcast/model.rs` says only
  /// those two values are ever written — `ended` exists so a row written by
  /// something else still decodes. So `status=ended` would be a filter for a
  /// value that does not occur, and "has ended" is expressed by `end_time`,
  /// which the endpoint cannot filter on. `inactive` is the closest the wire
  /// gets, and it carries drafts and scheduled broadcasts with it, so the feed
  /// drops whatever has not ended — see `BroadcastFeedDataSource`.
  ///
  /// Ordering is `created_at desc` because the wire has no other key, so a
  /// broadcast that ran long and ended a minute ago can sit below one created
  /// later and ended earlier. The socket prepends an end as it happens, which
  /// is what the section mostly shows; a feed that only reads history is when
  /// the difference is visible.
  const new recentlyLive({this.limit = defaultLimit})
    : type = BroadcastsType.recentlyLive,
      status = BroadcastStatus.inactive,
      creatorId = null,
      order = BroadcastOrder.newest,
      cursor = null;

  /// The live broadcasts for this viewer.
  ///
  /// Today that is every live broadcast: the endpoint filters by one
  /// `creatorId` and has no notion of "the accounts I follow", so there is
  /// nothing narrower to ask for. The type still travels with the query, so the
  /// socket and the screen behave as the section means to.
  const new forYou({this.limit = defaultLimit})
    : type = BroadcastsType.forYou,
      status = BroadcastStatus.active,
      creatorId = null,
      order = BroadcastOrder.newest,
      cursor = null;

  /// The accounts the viewer follows, as a live list.
  ///
  /// [BroadcastsType.subscriptions] is the one gap with no stand-in: what the
  /// tab needs is a filter over a *set* of creators, and the endpoint takes one
  /// `creatorId`. Until the API joins the follow table (or takes a
  /// `following=true`), this answers with the same rows as `nowLive` — the tab
  /// is honest about its heading, the list is not yet about subscriptions.
  const new subscriptions({this.limit = defaultLimit})
    : type = BroadcastsType.subscriptions,
      status = BroadcastStatus.active,
      creatorId = null,
      order = BroadcastOrder.newest,
      cursor = null;

  /// One account's broadcasts — a profile's list.
  const new byCreator(
    this.creatorId, {
    this.status,
    this.order = BroadcastOrder.newest,
    this.limit = defaultLimit,
  }) : type = null,
       cursor = null;

  /// The query a section draws.
  ///
  /// [BroadcastsType] is what a tab bar or a "See all" link carries, so this is
  /// the one place that says which filters a section means.
  factory section(
    BroadcastsType type, {
    int limit = defaultLimit,
  }) => switch (type) {
    BroadcastsType.forYou => BroadcastQuery.forYou(limit: limit),
    BroadcastsType.nowLive => BroadcastQuery.nowLive(limit: limit),
    BroadcastsType.recentlyLive => BroadcastQuery.recentlyLive(limit: limit),
    BroadcastsType.subscriptions => BroadcastQuery.subscriptions(limit: limit),
  };

  /// Reads a query back from [parameters], as a link or a tab leaves it.
  ///
  /// Anything unreadable is left out rather than defaulted to a wrong filter,
  /// so a hand-edited URL opens the section it names and no more.
  factory fromRouter(Map<String, dynamic> parameters) {
    final type = BroadcastsType.fromWire(parameters['type'] as String?);

    return BroadcastQuery(
      type: type,
      status: _statusFromWire(parameters['status'] as String?),
      creatorId: parameters['creatorId'] as String?,
      order: BroadcastOrder.values.firstWhere(
        (order) => order.wire == parameters['order'],
        orElse: () => BroadcastOrder.newest,
      ),
      limit:
          int.tryParse((parameters['limit'] as String?) ?? '') ?? defaultLimit,
    );
  }

  /// What the API clamps a page to when a request does not say.
  static const int defaultLimit = 20;

  /// What the endpoint clamps a page to, at most.
  static const int maxLimit = 100;

  /// Which list this is, for the socket and the screen. Never sent.
  final BroadcastsType? type;

  /// Only broadcasts in this status.
  final BroadcastStatus? status;

  /// Only this creator's broadcasts.
  final String? creatorId;

  /// Which way the `created_at` keyset walks.
  final BroadcastOrder order;

  /// The row the page starts after. Null asks for the first page.
  final String? cursor;

  /// Items per page. The API clamps to `[1, 100]`.
  final int limit;

  /// Whether anything narrows the rows.
  ///
  /// The type and the direction shape a list rather than filter it, so neither
  /// counts: a `forYou` feed with nothing else set is still "everything".
  bool get hasFilters => status != null || creatorId != null;

  /// Whether this is the first page.
  bool get isFirstPage => cursor == null;

  /// The same filters, asking for the page [cursor] names.
  ///
  /// A method rather than `copyWith` because `copyWith` cannot tell an argument
  /// that was left out from one that is null, and "no cursor" is exactly what
  /// the first page means: a refresh built with `copyWith(cursor: null)` would
  /// quietly ask for the page it already has.
  BroadcastQuery withCursor(String? cursor) =>
      copyWith(cursor: cursor, replaceCursor: true);

  BroadcastQuery copyWith({
    BroadcastsType? type,
    BroadcastStatus? status,
    String? creatorId,
    BroadcastOrder? order,
    String? cursor,
    int? limit,
    bool replaceCursor = false,
  }) => BroadcastQuery(
    type: type ?? this.type,
    status: status ?? this.status,
    creatorId: creatorId ?? this.creatorId,
    order: order ?? this.order,
    cursor: replaceCursor ? cursor : (cursor ?? this.cursor),
    limit: limit ?? this.limit,
  );

  /// Update search keywords and reset to first page
  BroadcastQuery updateKeywords(String? newKeywords) {
    // TODO(gettoknowdavid): implement sentinel for the copyWith method to
    // handle the case where the properties are replaced with null
    //
    // TODO(gettoknowdavid): implement keywords. Seems like it is not event
    // implemented on the backend
    return copyWith(
      // keywords: newKeywords,
      // cursor: null,
    );
  }

  /// The query as request parameters: exactly what the endpoint reads, with the
  /// client-side `type` left out and unset fields dropped.
  Map<String, dynamic> toQueryParameters() => {
    'creatorId': ?creatorId,
    'status': ?status?.wire,
    'order': order.wire,
    'cursor': ?cursor,
    'limit': limit,
  };

  /// The query as a URL the router can open: how a section survives a tap on
  /// "See all" and how a tab is restored from the address.
  ///
  /// The cursor is not here — it is a position in a list, not a filter — so a
  /// link always opens a section at its first page.
  Map<String, dynamic> toRouterParams() => {
    'type': ?type?.wire,
    'status': ?status?.wire,
    'creatorId': ?creatorId,
    'order': order.wire,
  };

  static BroadcastStatus? _statusFromWire(String? wire) {
    if (wire == null) return null;
    for (final status in BroadcastStatus.values) {
      if (status.wire == wire) return status;
    }
    return null;
  }

  @override
  List<Object?> get props => [type, status, creatorId, order, cursor, limit];
}
