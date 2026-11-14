import 'package:meno_mobile/_routing/_routing.dart';

/// The tabs Discover's shell draws, in order.
///
/// `suggestedAccounts` is deliberately absent: the tab it would be has no feed
/// behind it. Profiles are found by searching (`GET /users?q=`), not browsed as
/// a suggestion list, and the API has no endpoint that suggests anybody — so a
/// tab that claimed to would be showing something it had invented.
enum Filter {
  /// Live and recently live broadcasts together.
  all,

  /// Everything on air now.
  nowLive,

  /// Everything that has ended, most recent first.
  recentlyLive;

  /// The text the chip shows.
  String get label => switch (this) {
    Filter.all => 'All',
    Filter.nowLive => 'Now Live',
    Filter.recentlyLive => 'Recently Live',
  };

  /// The screen this tab is.
  ///
  /// The chip does not switch a local page index: it navigates, so the address
  /// names the tab, a deep link opens it, and the bottom bar's branch keeps the
  /// three of them apart.
  String get route => switch (this) {
    Filter.all => MRoutes.discoverAllTab,
    Filter.nowLive => MRoutes.discoverNowLiveTab,
    Filter.recentlyLive => MRoutes.discoverRecentlyLiveTab,
  };

  /// The tab [location] names, or null when it names none of them.
  ///
  /// Read from the address rather than remembered, so the chip bar and the
  /// router cannot disagree after a deep link or a "See all" from Home.
  static Filter? fromLocation(String location) {
    for (final filter in Filter.values) {
      if (filter.route == location) return filter;
    }
    return null;
  }
}
