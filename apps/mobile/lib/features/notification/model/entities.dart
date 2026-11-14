import 'package:meno_mobile/features/notification/model/proxies/notification_proxy.dart';

/// The kinds of notification this build can draw a card for.
///
/// The API's `typeCode` is the snake_case wire name; [value] is the camelCase
/// spelling the payload carried before. A code outside these three is refused
/// by `fromJson`, so the decoder skips that row instead of drawing a card with
/// no copy and nothing to open.
enum NotificationType {
  /// Somebody subscribed to this account.
  userSubscribed('userSubscribed'),

  /// Somebody added this account as a co-host of their broadcast.
  addedAsCoHost('addedAsCoHost'),

  /// A followee went live.
  liveBroadcastStarted('liveBroadcastStarted');

  /// Creates a type with the spelling it is stored under.
  new(this.value);

  /// Reads the API's `typeCode`, in either spelling the deployment has used.
  ///
  /// # Errors
  ///
  /// Throws a [FormatException] when [json] names a type this build cannot
  /// draw. `CursorPage` reports the skip and keeps the rest of the page.
  factory fromJson(String? json) {
    return switch (json) {
      'user_subscribed' || 'userSubscribed' => userSubscribed,
      'added_as_co_host' || 'addedAsCoHost' => addedAsCoHost,
      'live_broadcast_started' ||
      'liveBroadcastStarted' => liveBroadcastStarted,
      _ => throw FormatException('Unknown notification type: $json'),
    };
  }

  /// The camelCase spelling of this type.
  final String value;
}

/// The copy and images one notification card draws.
///
/// The API sends these flat on the notification (`title`, `body`, `imageUrl`,
/// `actor`); the DTO fills the fields the cards read, so a card never has to
/// know which type it is holding to find a name or an avatar.
final class NotificationContent {
  /// Creates the content of a notification.
  const new({
    this.subscriberId,
    this.subscriberName,
    this.subscriptionId,
    this.subscriberImageUrl,
    this.cohostId,
    this.broadcastId,
    this.broadcastTitle,
    this.cohostFullName,
    this.cohostImageUrl,
    this.broadcastCreator,
    this.broadcastImageUrl,
    this.id,
    this.title,
    this.imageUrl,
  });

  /// The account that subscribed.
  final String? subscriberId;

  /// The subscriber's display name.
  final String? subscriberName;

  /// The subscription row, when the API names one.
  final String? subscriptionId;

  /// The subscriber's avatar URL.
  final String? subscriberImageUrl;

  /// The account that added this one as a co-host.
  final String? cohostId;

  /// The broadcast the notification refers to.
  final String? broadcastId;

  /// The broadcast's rendered title.
  final String? broadcastTitle;

  /// The co-host's display name.
  final String? cohostFullName;

  /// The co-host's avatar URL.
  final String? cohostImageUrl;

  /// The name of whoever went live.
  final String? broadcastCreator;

  /// The broadcast's cover image.
  final String? broadcastImageUrl;

  /// The broadcast id, in the field the live card reads it from.
  final String? id;

  /// The headline the card shows.
  final String? title;

  /// The image the card shows.
  final String? imageUrl;
}

/// One notification in this account's inbox.
///
/// The wire keeps its text and its ids flat; [content] is the shape the cards
/// read, filled by the DTO from those flat fields.
final class Notification {
  /// Creates a notification.
  const new({
    this.read = false,
    this.id,
    this.type,
    this.content,
    this.createdAt,
    this.updatedAt,
  });

  /// The API's primary key.
  final String? id;

  /// Which card draws it.
  final NotificationType? type;

  /// Whether the recipient has read it.
  final bool read;

  /// The copy and images the card and the toast read.
  final NotificationContent? content;

  /// When the API created it.
  final DateTime? createdAt;

  /// When it last changed, when the API says.
  final DateTime? updatedAt;
}

/// One row of the grouped inbox list: either a heading or a notification.
sealed class NotificationListItem {
  /// Creates a list item.
  const new();
}

/// A heading between two runs of notifications.
final class NotificationSectionHeader extends NotificationListItem {
  /// Creates a heading with [label].
  const new(this.label);

  /// The heading's text.
  final String label;
}

/// One notification in the grouped list.
final class NotificationListEntry extends NotificationListItem {
  /// Creates an entry around [proxy].
  const new(this.proxy);

  /// The notification to draw.
  final NotificationProxy proxy;
}

/// The socket's announcement that a notification was created.
///
/// The frame carries `{userId, title, body, timestamp}` and nothing else — no
/// id, no type, no deep link. Those live on the stored notification, so this
/// signal says only that something arrived; the manager re-reads the inbox
/// rather than building a row from copy the list cannot act on.
final class NotificationSignal {
  /// Creates a signal from the frame's copy.
  const new({required this.title, required this.body});

  /// The title the frame carried.
  final String title;

  /// The body the frame carried.
  final String body;
}
