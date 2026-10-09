import 'package:flutter/material.dart';
import 'package:meno_ui/meno_ui.dart';

/// A small circular widget representing a dot.
///
/// This widget displays a colored circle with a customizable size and color.
class MDot extends StatelessWidget {
  /// Creates a new `MDot` widget.
  ///
  /// * `dimension`: The size of the dot (default: 4.0).
  /// * `color`: The color of the dot (defaults to [MColor.grey500]).
  const MDot({super.key, this.dimension = 4, this.color});

  /// The size of the dot.
  final double dimension;

  /// The color of the dot.
  final Color? color;

  @override
  Widget build(BuildContext context) {
    final dotColor = color ?? MColor.grey500;
    return Container(
      width: dimension,
      height: dimension,
      decoration: BoxDecoration(color: dotColor, shape: BoxShape.circle),
    );
  }
}
