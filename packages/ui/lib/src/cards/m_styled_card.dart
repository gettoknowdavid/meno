import 'package:material_ui/material_ui.dart';

/// An abstract base class for a styled card widget.
///
/// This class represents a card that can display a child widget, an optional
/// image, title, and subtitle. It also supports a tap gesture callback.
/// Subclasses should provide specific implementations for how the card is
/// styled and rendered.
///
/// Example usage:
/// ```dart
/// MStyledCard(
///   child: Text('Card Content'),
///   imageUrl: 'https://example.com/image.jpg',
///   title: 'Card Title',
///   subtitle: 'Card Subtitle',
///   onTap: () {
///     // Handle tap event
///   },
/// );
/// ```
abstract class MStyledCard extends StatelessWidget {
  /// Creates an instance of [MStyledCard].
  ///
  /// Parameters:
  /// - [child]: The primary content of the card.
  /// - [key]: An optional key to identify the widget.
  /// - [imageUrl]: An optional URL for an image to be displayed in the card.
  /// - [title]: An optional title to be displayed on the card.
  /// - [subtitle]: An optional subtitle to be displayed on the card.
  /// - [onTap]: An optional callback function to be invoked when the card is
  /// tapped.
  /// - [loading]: A boolean to show the loading state of the card.
  const new({
    required this.child,
    super.key,
    this.imageUrl,
    this.title,
    this.subtitle,
    this.onTap,
    this.loading = false,
  });

  /// The primary content of the card.
  final Widget child;

  /// The URL of an image to be displayed in the card.
  final String? imageUrl;

  /// The title to be displayed on the card.
  final String? title;

  /// The subtitle to be displayed on the card.
  final String? subtitle;

  /// A callback function to be invoked when the card is tapped.
  final VoidCallback? onTap;

  /// A boolean to show the loading state of the card.
  final bool loading;

  @override
  Widget build(BuildContext context) => child;
}
