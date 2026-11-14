import 'package:flutter/widgets.dart';
import 'package:meno_ui/src/cards/cards.dart';

/// A namespace for creating styled cards with different types of content.
///
/// Provides factory constructors for creating specific types of cards,
/// such as live cards and recently live cards.
///
/// To create a live card or a recently live card, use the [MCard.live] or
/// [MCard.recentlyLive] factory methods.
///
/// Example usage:
/// ```dart
/// MCard.liveI
///   title: 'Live Event Title',
///   host: 'Host Name',
///   imageUrl: 'https://example.com/image.jpg',
///   liveCount: 1234,
///   onTap: () {
///     // Handle card tap
///   },
/// );
/// ```
@immutable
final class MCard {
  /// Prevents instantiation: this type is a namespace for card factories.
  const new _();

  /// Creates a live card.
  ///
  /// Parameters:
  /// - [key]: An optional key to identify the widget.
  /// - [title]: The title of the live content.
  /// - [host]: The name of the host of the live content.
  /// - [imageUrl]: An optional URL for an image to be displayed in the card.
  /// - [liveCount]: An optional count of live viewers or participants.
  /// - [onTap]: An optional callback function to be invoked when the card is
  /// tapped.
  static Widget live({
    Key? key,
    String? title,
    String? host,
    String? imageUrl,
    int? liveCount,
    VoidCallback? onTap,
  }) {
    return MLiveCard(
      title: title,
      host: host,
      imageUrl: imageUrl,
      liveCount: liveCount,
      onTap: onTap,
      key: key,
    );
  }

  /// Creates a recently live card.
  ///
  /// Parameters:
  /// - [key]: An optional key to identify the widget.
  /// - [title]: The title of the recently live content.
  /// - [host]: An optional name of the host of the recently live content.
  /// - [imageUrl]: An optional URL for an image to be displayed in the card.
  /// - [onTap]: An optional callback function to be invoked when the card is
  /// tapped.
  static Widget recentlyLive({
    Key? key,
    String? title,
    String? host,
    String? imageUrl,
    VoidCallback? onTap,
  }) {
    return MRecentlyLiveCard(
      title: title,
      host: host,
      imageUrl: imageUrl,
      onTap: onTap,
      key: key,
    );
  }
}
