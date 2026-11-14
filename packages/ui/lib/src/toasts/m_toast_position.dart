import 'package:material_ui/material_ui.dart';

/// Where a toast is anchored in the overlay and the direction it enters from.
///
/// The primary constructor pairs an [alignment] — where the toast comes to rest
/// inside the overlay — with a [translation], the fraction of the toast's own
/// size it is offset by while hidden. A top-anchored toast therefore slides
/// down into place while a centre-left toast slides in from the left, so one
/// toast can enter from any edge or corner without extra code.
///
/// Example usage:
/// ```dart
/// context.showToast('Saved', position: MToastPosition.topLeft);
/// ```
enum MToastPosition(
  /// Where the toast comes to rest inside the overlay.
  final Alignment alignment,

  /// The hidden offset, as a fraction of the toast's own size.
  final Offset translation,
) {
  /// The top-left corner, entering diagonally from that corner.
  topLeft(Alignment.topLeft, Offset(-1, -1)),

  /// Top-centre, entering from above.
  topCenter(Alignment.topCenter, Offset(0, -1)),

  /// The top-right corner, entering diagonally from that corner.
  topRight(Alignment.topRight, Offset(1, -1)),

  /// The centre-left edge, entering from the left.
  centerLeft(Alignment.centerLeft, Offset(-1, 0)),

  /// Dead centre, scaling up into place rather than sliding.
  center(Alignment.center, Offset.zero),

  /// The centre-right edge, entering from the right.
  centerRight(Alignment.centerRight, Offset(1, 0)),

  /// The bottom-left corner, entering diagonally from that corner.
  bottomLeft(Alignment.bottomLeft, Offset(-1, 1)),

  /// Bottom-centre, entering from below.
  bottomCenter(Alignment.bottomCenter, Offset(0, 1)),

  /// The bottom-right corner, entering diagonally from that corner.
  bottomRight(Alignment.bottomRight, Offset(1, 1));

  /// Whether toasts at this position are ordered newest-first.
  ///
  /// Top-anchored stacks grow downwards, so the newest toast should sit closest
  /// to the top edge. Every other stack grows towards its anchor, so the newest
  /// toast sits last.
  bool get newestFirst => alignment.y < 0;

  /// Whether this position scales into place instead of sliding.
  bool get scales => translation == Offset.zero;
}
