import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/profile/model/entities.dart';
import 'package:meno_mobile/features/profile/model/proxies/user_profile_proxy.dart';

/// The live profile proxies, one per account, for as long as somebody watches.
///
/// # Why a repository rather than a manager each
///
/// An account's profile is drawn in several places at once — their profile
/// page, a broadcast card, the header of a room they are hosting — and each of
/// those wants the *same* answer about whether the viewer follows them. Giving
/// every screen its own proxy would give the app as many answers as it has
/// screens, and following someone on their page would leave the card behind it
/// stale.
///
/// # Keyed by the requested account
///
/// The map is keyed by the id the caller *asked for*, fixed when the proxy is
/// acquired. The audit's defect was the opposite: every cold profile acquired
/// one shared empty placeholder, the fetch then wrote the fetched id into it,
/// and the release looked up a key nobody had ever inserted (H13). Here the key
/// and the proxy's identity cannot drift, because neither can change.
///
/// # Reference counting
///
/// A proxy is created when the first holder acquires it and disposed when the
/// last one releases it; the holder is whatever brought it on screen, which in
/// practice is a page's manager. Counting is not decoration: without it, the
/// first page to close would dispose a proxy a second page is still drawing.
///
/// Deliberately not an interface: there is one of these, it is a map and a
/// count, and a fake of it would test the fake.
final class UserProfileRepository {
  final _proxies = <String, UserProfileProxy>{};

  /// The proxy for [subjectId], for one more holder.
  ///
  /// An id that is not a UUID is refused rather than turned into a key that
  /// belongs to nobody.
  UserProfileProxy acquire(Id subjectId) {
    if (!subjectId.isValid) {
      throw const OwnerViolationException(
        message: 'A profile proxy needs the account it is about.',
      );
    }

    final key = subjectId.getOrCrash;
    final proxy = _proxies[key] ??= UserProfileProxy(subjectId);
    proxy.retain();
    return proxy;
  }

  /// Gives up one hold on [proxy], disposing it when it was the last.
  ///
  /// The lookup uses the proxy's own subject — never the id of whatever was
  /// last fetched — and a proxy this repository no longer owns is ignored, so
  /// a stray second release cannot dispose a notifier somebody is drawing.
  void release(UserProfileProxy proxy) {
    final key = proxy.subjectId.getOrCrash;
    if (!identical(_proxies[key], proxy)) return;
    if (!proxy.release()) return;

    _proxies.remove(key);
    proxy.dispose();
  }

  /// Adopts [profile] as [proxy]'s target.
  ///
  /// Throws when the answer names a different account: a mismatched response is
  /// a bug worth hearing about, not something to paint.
  void apply(UserProfileProxy proxy, Profile profile) => proxy.target = profile;

  /// How many accounts currently have a live proxy.
  ///
  /// For tests and diagnostics; the app never needs the number.
  int get length => _proxies.length;
}
