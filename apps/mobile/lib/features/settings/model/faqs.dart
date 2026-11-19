/// The questions the FAQs screen answers, and the search over them.
///
/// Bundled with the app rather than fetched: help content has to work offline
/// (the same reason the legal documents are bundled), it changes with the
/// build it describes, and it needs no endpoint that could be down when
/// somebody is already confused.
///
/// Answers are deliberately short and honest about what the build does not do
/// yet. A FAQ that promises recording, listen-later or push notifications
/// would be the dead end this screen exists to remove.
library;

/// One question and its answer.
class FaqEntry {
  /// Creates an entry.
  const new({
    required this.question,
    required this.answer,
    this.keywords = const [],
  });

  /// The question, in the user's words.
  final String question;

  /// The answer, short enough to read on a phone without scrolling twice.
  final String answer;

  /// Extra terms the search matches that neither field contains.
  ///
  /// "tone" finds the notification-sound answer, "audio" finds the recording
  /// one — the words a person would actually try.
  final List<String> keywords;

  /// Whether this entry should be shown for [query].
  ///
  /// Case-insensitive and substring, so "broad cast" still finds "broadcast"
  /// only if the phrase appears — that is the point: a partial word is the
  /// normal way to search.
  bool matches(String query) {
    final needle = query.trim().toLowerCase();
    if (needle.isEmpty) return true;

    return question.toLowerCase().contains(needle) ||
        answer.toLowerCase().contains(needle) ||
        keywords.any((word) => word.toLowerCase().contains(needle));
  }
}

/// A group of questions shown under one heading.
class FaqSection {
  /// Creates a section.
  const new({required this.title, required this.entries});

  /// The heading, e.g. "Broadcasting".
  final String title;

  /// The questions in this group, in the order they are shown.
  final List<FaqEntry> entries;
}

/// Whether any section contains an entry for [query].
bool faqHasResults(String query) => searchFaqSections(query).isNotEmpty;

/// The sections that contain [query], with non-matching entries removed.
///
/// Empty sections are dropped, so the list never shows a heading over nothing.
/// An empty query returns everything, which is what an empty search box means.
List<FaqSection> searchFaqSections(String query) {
  return [
    for (final section in faqSections)
      if (section.entries.any((entry) => entry.matches(query)))
        FaqSection(
          title: section.title,
          entries: section.entries
              .where((entry) => entry.matches(query))
              .toList(growable: false),
        ),
  ];
}

/// Everything the FAQs screen shows when the search box is empty.
const List<FaqSection> faqSections = [
  FaqSection(
    title: 'Getting started',
    entries: [
      FaqEntry(
        question: 'What is Menō?',
        answer:
            'Menō is a live audio platform. You can listen to broadcasts as '
            'they happen, host your own, take notes, and read the Bible — all '
            'in one app.',
        keywords: ['meno', 'app', 'about'],
      ),
      FaqEntry(
        question: 'Do I need an account to listen?',
        answer:
            'Yes. Listening, notes, bookmarks and broadcasts all belong to an '
            'account, so signing up first is how everything stays in sync.',
        keywords: ['sign up', 'register', 'guest'],
      ),
      FaqEntry(
        question: 'How much does Menō cost?',
        answer:
            'It is free to use. Nothing on the current build asks for payment '
            'or a subscription.',
        keywords: ['price', 'paid', 'subscription', 'billing'],
      ),
      FaqEntry(
        question: 'What do I need to listen?',
        answer:
            'A phone or tablet with an internet connection. Wi-Fi is worth it '
            'for a long broadcast — live audio is continuous, unlike a feed '
            'that loads once.',
        keywords: ['data', 'internet', 'wifi'],
      ),
      FaqEntry(
        question: 'Is Menō audio only?',
        answer:
            'Yes. Broadcasts are live audio. Co-hosts speak; listeners hear '
            'the room and take part in chat.',
        keywords: ['video', 'camera', 'visual'],
      ),
    ],
  ),
  FaqSection(
    title: 'Account and sign-in',
    entries: [
      FaqEntry(
        question: 'How do I create an account?',
        answer:
            'Tap Get started, enter your name, email and a password, accept '
            'the terms, and confirm the six-digit code we email you.',
        keywords: ['register', 'sign up', 'new'],
      ),
      FaqEntry(
        question: 'I did not get the verification code',
        answer:
            'Wait a minute, check your spam folder, then tap Resend. Codes '
            'expire, so an old one in an old email will not work — use the '
            'newest message.',
        keywords: ['otp', 'code', 'email', 'verify'],
      ),
      FaqEntry(
        question: 'I forgot my password',
        answer:
            'Tap Forgot password on the sign-in screen. We email a code, you '
            'set a new password, and you are back in without losing anything.',
        keywords: ['reset', 'locked out', 'password'],
      ),
      FaqEntry(
        question: 'Can I sign in with Google?',
        answer:
            'Yes, where the button is offered on the sign-in screen. It '
            'creates or opens the same account as an email sign-in with that '
            'address.',
        keywords: ['google', 'oauth', 'sso'],
      ),
      FaqEntry(
        question: 'Can I change my email address?',
        answer:
            'Not in the app yet. Your email is the identity you sign in with, '
            'so changing it needs a person. Contact support and we will do it '
            'with you.',
        keywords: ['email', 'change', 'update'],
      ),
      FaqEntry(
        question: 'How do I change my password?',
        answer:
            'Settings → Security → Change password. You will need your '
            'current password.',
        keywords: ['password', 'security', 'change'],
      ),
      FaqEntry(
        question: 'How do I delete my account?',
        answer:
            'Settings → Delete Account. It asks once, then removes your '
            'profile, sessions and content from the app. This cannot be '
            'undone.',
        keywords: ['delete', 'remove', 'close', 'gdpr'],
      ),
      FaqEntry(
        question: 'Can I use Menō on more than one device?',
        answer:
            'Yes. Sign in on each device with the same account. Notes, '
            'bookmarks, settings and your profile follow the account, not the '
            'phone.',
        keywords: ['devices', 'sync', 'tablet', 'multiple'],
      ),
    ],
  ),
  FaqSection(
    title: 'Broadcasting',
    entries: [
      FaqEntry(
        question: 'Who can start a broadcast?',
        answer:
            'Any signed-in account. Tap the create button, add a title and a '
            'cover image, and go live when you are ready.',
        keywords: ['host', 'start', 'create', 'go live'],
      ),
      FaqEntry(
        question: 'Can I schedule a broadcast?',
        answer:
            'Yes. Pick a date and time while creating it. A scheduled '
            'broadcast appears in the feed so people can plan to join.',
        keywords: ['schedule', 'later', 'upcoming'],
      ),
      FaqEntry(
        question: 'What happens when I end a broadcast?',
        answer:
            'Everyone in the room is told it ended, the room closes, and the '
            'broadcast shows as ended in your profile. It cannot be reopened '
            'as live.',
        keywords: ['end', 'stop', 'finish'],
      ),
      FaqEntry(
        question: 'Can I go live again with the same broadcast?',
        answer:
            'No. An ended broadcast stays ended, so start a new one — that '
            'keeps the record of what was said when honest.',
        keywords: ['restart', 'again', 'reuse'],
      ),
      FaqEntry(
        question: 'Can I record a broadcast, or listen to one later?',
        answer:
            'Not yet. Recording, publish and listen-back are not in this '
            'build, so those controls are hidden rather than shown and broken.',
        keywords: ['record', 'recording', 'replay', 'download', 'listen later'],
      ),
      FaqEntry(
        question: 'Can listeners speak?',
        answer:
            'Listeners hear the room and can chat. Speaking is for the host '
            'and the co-hosts the host invites.',
        keywords: ['microphone', 'speak', 'talk', 'mute'],
      ),
      FaqEntry(
        question: 'What does a cover image do?',
        answer:
            'It is the picture people see in the feed and on the details '
            'screen, so a clear one is worth the minute it takes.',
        keywords: ['image', 'cover', 'thumbnail', 'photo'],
      ),
    ],
  ),
  FaqSection(
    title: 'Listening',
    entries: [
      FaqEntry(
        question: 'How do I join a live broadcast?',
        answer:
            'Open the home or discover feed and tap a broadcast marked live. '
            'The room opens and audio starts.',
        keywords: ['join', 'listen', 'live'],
      ),
      FaqEntry(
        question: 'How do I know when someone goes live?',
        answer:
            'The notifications inbox shows it, and the app can alert you '
            'while it is open. Push alerts when the app is closed are not '
            'switched on yet.',
        keywords: ['alert', 'notify', 'notification', 'follow'],
      ),
      FaqEntry(
        question: 'What if my connection drops mid-broadcast?',
        answer:
            'The app reconnects and rejoins the room you were in. If it does '
            'not, go back to the broadcast from the feed.',
        keywords: ['offline', 'disconnect', 'reconnect', 'network'],
      ),
    ],
  ),
  FaqSection(
    title: 'Co-hosting',
    entries: [
      FaqEntry(
        question: 'What is a co-host?',
        answer:
            'Someone the host invites to speak in a live room. Co-hosts can '
            'talk and take part; the host keeps the controls.',
        keywords: ['cohost', 'co-host', 'guest', 'panel'],
      ),
      FaqEntry(
        question: 'How do I invite a co-host?',
        answer:
            'In a live room you host, open the participants sheet, search for '
            'the person, and send an invitation.',
        keywords: ['invite', 'add', 'cohost'],
      ),
      FaqEntry(
        question: 'How do I decline or leave as a co-host?',
        answer:
            'Decline the invitation when it arrives, or leave the room. '
            'Either way the broadcast carries on without you.',
        keywords: ['decline', 'leave', 'exit'],
      ),
    ],
  ),
  FaqSection(
    title: 'Chat and reactions',
    entries: [
      FaqEntry(
        question: 'Can I edit or delete a message?',
        answer:
            'Yes, your own messages. Edited messages are marked as edited, and '
            'a deleted one leaves the conversation rather than pretending it '
            'was never sent.',
        keywords: ['chat', 'message', 'edit', 'delete'],
      ),
      FaqEntry(
        question: 'How do reactions work?',
        answer:
            'React to a message from its menu. Reactions are counted per '
            'message, so a busy room does not lose them.',
        keywords: ['react', 'emoji', 'like'],
      ),
      FaqEntry(
        question: 'How do I report or block someone?',
        answer:
            'Report a message or a profile from its options. Blocking is '
            'honoured by chat and follows, so a blocked person cannot reach '
            'you through either.',
        keywords: ['report', 'block', 'abuse', 'harassment', 'moderation'],
      ),
    ],
  ),
  FaqSection(
    title: 'Notes, bookmarks and the Bible',
    entries: [
      FaqEntry(
        question: 'How do I bookmark a broadcast?',
        answer:
            'Tap the bookmark control on a broadcast. Bookmarks are kept with '
            'the account, so they are on every device you sign in to.',
        keywords: ['bookmark', 'save', 'favourite'],
      ),
      FaqEntry(
        question: 'How do notes and folders work?',
        answer:
            'Write a note, then put it in a folder to keep subjects apart. '
            'Both are searchable.',
        keywords: ['notes', 'folders', 'organise'],
      ),
      FaqEntry(
        question: 'Do notes sync between devices?',
        answer:
            'Yes. Notes are stored on your account and sync when the app has a '
            'connection; anything written offline is sent when it returns.',
        keywords: ['sync', 'offline', 'devices'],
      ),
      FaqEntry(
        question: 'Can I read the Bible offline?',
        answer:
            'Download a translation from the Bible translations screen and it '
            'stays on the device for reading without a connection.',
        keywords: ['bible', 'offline', 'download', 'translation'],
      ),
      FaqEntry(
        question: 'Where does the Bible text come from?',
        answer:
            'Each translation is credited to its publisher. Settings → '
            'Copyright & Credits lists them.',
        keywords: ['bible', 'credits', 'licence', 'publisher'],
      ),
    ],
  ),
  FaqSection(
    title: 'Notifications and settings',
    entries: [
      FaqEntry(
        question: 'Why do notifications only arrive while the app is open?',
        answer:
            'Push delivery is not switched on in this build, so the inbox is '
            'fed while the app runs. The settings screen says so, and the '
            'toggles apply as soon as push lands.',
        keywords: ['push', 'notification', 'background', 'alert'],
      ),
      FaqEntry(
        question: 'How do I change the theme?',
        answer:
            'Settings → Theme, and choose System, Light or Dark. System '
            'follows the phone.',
        keywords: ['theme', 'dark', 'light', 'appearance'],
      ),
      FaqEntry(
        question: 'Can I change the language?',
        answer:
            'Not yet — the app is English-only for now. Language is a planned '
            'setting, not a hidden one.',
        keywords: ['language', 'locale', 'translation', 'english'],
      ),
      FaqEntry(
        question: 'Which notifications can I turn off?',
        answer:
            'Settings → Notifications controls app, push and email alerts, '
            'plus each kind: a broadcast starting, a new subscriber, a co-host '
            'invitation and a scheduled broadcast.',
        keywords: ['settings', 'preferences', 'notifications', 'email'],
      ),
    ],
  ),
  FaqSection(
    title: 'Privacy, safety and support',
    entries: [
      FaqEntry(
        question: 'Where are the privacy policy and terms?',
        answer:
            'Settings → About Menō, or the link on the sign-up screen. Both '
            'documents ship with the app, so they are readable offline.',
        keywords: ['privacy', 'terms', 'legal', 'policy'],
      ),
      FaqEntry(
        question: 'Who can see my profile?',
        answer:
            'Your name, username, avatar and the broadcasts you have hosted '
            'are public to signed-in accounts. Your email and settings are '
            'not.',
        keywords: ['profile', 'visibility', 'public', 'private'],
      ),
      FaqEntry(
        question: 'How do I report a problem with the app?',
        answer:
            'Settings → About Menō → Report an app problem, or email support '
            'directly from that sheet. Tell us what you expected, what '
            'happened, and which device you are on.',
        keywords: ['support', 'contact', 'bug', 'help', 'email'],
      ),
      FaqEntry(
        question: 'What happens to my data when I delete my account?',
        answer:
            'Your profile, sessions and content are removed from the app and '
            'the account can no longer sign in. If you want a copy first, say '
            'so in a support message before deleting.',
        keywords: ['delete', 'data', 'privacy', 'export'],
      ),
    ],
  ),
];
