/// Centralised route registry for the Meno mobile app.
///
/// Two kinds of constant are exposed for every route:
///
/// * A **path pattern** (e.g. [broadcastPattern]) using go_router `:param`
///   placeholders. These are consumed when registering routes on the
///   `GoRouter` (i.e. `GoRoute(path: MRoutes.broadcastPattern, ...)`).
/// * A **builder** (e.g. [broadcastRoute]) that produces a concrete,
///   deep-link-safe path for navigation (`context.go(...)`) and sharing.
///
/// Paths are kebab-cased and hierarchical so that App Links / Universal Links
/// (`https://<host>/<path>`) map 1:1 onto in-app destinations. Use [deepLink]
/// to turn any route path into a shareable URL.
final class MRoutes {
  const new _();

  /// Canonical web host backing App Links / Universal Links.
  static const String deepLinkHost = 'https://meno.app';

  /// Turns an in-app [path] into a shareable https deep link.
  static String deepLink(String path) {
    if (path.isEmpty) return deepLinkHost;
    return path.startsWith('/') ? '$deepLinkHost$path' : '$deepLinkHost/$path';
  }

  /// Routes reachable while signed out. Used by the router redirect guard.
  static const List<String> publicRoutes = [
    onboarding,
    login,
    register,
    emailVerification,
    forgotPassword,
    resetPassword,
    resetPasswordOtp,
    createNewPassword,
    resetPasswordSuccess,
    liveSessionInit,
  ];

  static bool isPublic(String location) => publicRoutes.contains(location);

  static const String loading = '/loading';
  static const String onboarding = '/onboarding';

  static const String login = '/login';

  static String loginRoute({bool? implyLeading, bool? isPasswordOnly}) {
    final query = <String, String>{
      if (implyLeading != null) 'implyLeading': '$implyLeading',
      if (isPasswordOnly == true) 'isPasswordOnly': 'true',
    };
    return _withQuery(login, query);
  }

  static const String register = '/register';

  static String registerRoute({bool? implyLeading}) {
    final query = <String, String>{
      if (implyLeading != null) 'implyLeading': '$implyLeading',
    };
    return _withQuery(register, query);
  }

  static const String emailVerification = '/verify-email';

  static String emailVerificationRoute({String? email}) {
    return _withQuery(emailVerification, {'email': email});
  }

  static const String forgotPassword = '/forgot-password';

  static const String resetPassword = '/reset-password';

  static String resetPasswordRoute({String? email}) {
    return _withQuery(resetPassword, {'email': email});
  }

  static const String resetPasswordOtp = '/reset-password/otp';

  static String resetPasswordOtpRoute({String? email}) {
    return _withQuery(resetPasswordOtp, {'email': email});
  }

  static const String createNewPassword = '/reset-password/new';

  static String createNewPasswordRoute({String? email, String? code}) {
    return _withQuery(createNewPassword, {'email': email, 'code': code});
  }

  static const String resetPasswordSuccess = '/reset-password/success';

  static const String authGoogle = '/auth/google';

  static const String home = '/';
  static const String discover = '/discover';
  static const String discoverSearch = '/discover/search';

  static const String discoverAllTab = '/discover/all';
  static const String discoverNowLiveTab = '/discover/now-live';
  static const String discoverRecentlyLiveTab = '/discover/recently-live';
  static const String discoverSuggestedAccountsTab = '/discover/accounts';

  static const String nowLive = '/now-live';
  static const String recentlyLive = '/recently-live';
  static const String notifications = '/notifications';

  static const String profiles = '/profiles';
  static const String myProfile = '/profile';

  static const String userProfilePattern = '/users/:userId';

  static String userProfileRoute(String userId, {String? tab}) {
    return _withQuery('/users/${Uri.encodeComponent(userId)}', {'tab': tab});
  }

  static const String switchAccountPrefix = '/switch-account';
  static const String switchAccountPattern = '$switchAccountPrefix/:userId';

  static String switchAccountRoute(String userId) {
    return '/switch-account/${Uri.encodeComponent(userId)}';
  }

  /// Tab values accepted by [userProfileRoute]'s `tab` query parameter.
  static const String userProfileRecentTab = 'recent';
  static const String userProfileAllTab = 'all';
  static const String userProfileFavouritesTab = 'favourites';

  static const String settings = '/settings';
  static const String notificationSettings = '/settings/notifications';
  static const String securitySettings = '/settings/security';
  static const String about = '/settings/about';

  static const String broadcasts = '/broadcasts';
  static const String webCreateBroadcast = '/web/broadcast/create';

  /// Create a new broadcast (draft or scheduled).
  static const String broadcastEditor = '/broadcasts/new';

  /// Edit an existing broadcast.
  static const String broadcastEditorPattern = '/broadcasts/:broadcastId/edit';

  static String broadcastEditorRoute([String? broadcastId]) {
    if (broadcastId == null || broadcastId.isEmpty) return broadcastEditor;
    return '/broadcasts/${Uri.encodeComponent(broadcastId)}/edit';
  }

  static const String broadcastPattern = '/broadcasts/:broadcastId';

  static String broadcastRoute(String broadcastId) {
    return '/broadcasts/${Uri.encodeComponent(broadcastId)}';
  }

  /// Pre-stream lobby. Registered as a **modal** (`fullscreenDialog`, root
  /// navigator) so it can be deep-linked directly from a share link as well as
  /// pushed over the broadcast card that opened it.
  static const String preStreamPattern = '/broadcasts/:broadcastId/pre-stream';

  static String preStreamRoute(String broadcastId) {
    return '/broadcasts/${Uri.encodeComponent(broadcastId)}/pre-stream';
  }

  /// Live broadcast host/listener shell.
  static const String liveBroadcastPattern = '/broadcasts/:broadcastId/live';

  static String liveBroadcastRoute(String broadcastId) {
    return '/broadcasts/${Uri.encodeComponent(broadcastId)}/live';
  }

  static const String liveChatPattern = '/broadcasts/:broadcastId/live/chat';

  static String liveChatRoute(String broadcastId) {
    return '/broadcasts/${Uri.encodeComponent(broadcastId)}/live/chat';
  }

  static const String liveBiblePattern = '/broadcasts/:broadcastId/live/bible';

  static String liveBibleRoute(String broadcastId) {
    return '/broadcasts/${Uri.encodeComponent(broadcastId)}/live/bible';
  }

  static const String liveNotesPattern = '/broadcasts/:broadcastId/live/notes';

  static String liveNotesRoute(String broadcastId) {
    return '/broadcasts/${Uri.encodeComponent(broadcastId)}/live/notes';
  }

  static const String liveNoteEditorPattern =
      '/broadcasts/:broadcastId/live/notes/:noteId';

  static String liveNoteEditorRoute(
    String broadcastId, [
    String noteId = 'new',
  ]) {
    return '/broadcasts/${Uri.encodeComponent(broadcastId)}/live/notes/'
        '${Uri.encodeComponent(noteId)}';
  }

  static const String liveFoldersPattern =
      '/broadcasts/:broadcastId/live/folders';

  static String liveFoldersRoute(String broadcastId) {
    return '/broadcasts/${Uri.encodeComponent(broadcastId)}/live/folders';
  }

  static const String liveFolderPattern =
      '/broadcasts/:broadcastId/live/folders/:folderId';

  static String liveFolderRoute(String broadcastId, String folderId) {
    return '/broadcasts/${Uri.encodeComponent(broadcastId)}/live/folders/'
        '${Uri.encodeComponent(folderId)}';
  }

  /// Post-broadcast summary screen.
  static const String endedBroadcastPattern = '/broadcasts/:broadcastId/ended';

  static String endedBroadcastRoute(String broadcastId) {
    return '/broadcasts/${Uri.encodeComponent(broadcastId)}/ended';
  }

  /// The transition page that sits between the router and a live room.
  ///
  /// A room is opened asynchronously — the scope exists before it is ready — so
  /// the router shows this page in between and lets it wait on the scope. It is
  /// a named route so the pre-stream modal and the live shell can hand a client
  /// here by address rather than by reconstructing the path.
  static const String liveSessionInit = '/broadcasts/:broadcastId/live-session-initialization';

  static String liveSessionInitRoute(String broadcastId) {
    return '/broadcasts/${Uri.encodeComponent(broadcastId)}/live-session-initialization';
  }

  static const String notes = '/notes';
  static const String noteEditorPattern = '/notes/:noteId';

  static String noteEditorRoute([String noteId = 'new']) {
    return '/notes/${Uri.encodeComponent(noteId)}';
  }

  static const String folders = '/folders';
  static const String folderPattern = '/folders/:folderId';

  static String folderRoute(String folderId) {
    return '/folders/${Uri.encodeComponent(folderId)}';
  }

  static const String bible = '/bible';

  /// Appends [query] to [path], dropping null/empty values and percent-encoding
  /// each entry so the result is safe for deep links.
  static String _withQuery(String path, Map<String, String?> query) {
    final entries = query.entries
        .where((e) => e.value != null && e.value!.isNotEmpty)
        .map(
          (e) =>
              '${Uri.encodeQueryComponent(e.key)}='
              '${Uri.encodeQueryComponent(e.value!)}',
        )
        .toList();
    if (entries.isEmpty) return path;
    return '$path?${entries.join('&')}';
  }
}
