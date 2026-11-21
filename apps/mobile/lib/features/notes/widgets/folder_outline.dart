import 'package:material_ui/material_ui.dart';

/// The folder outline: bottom corners rounded, and a notch in the top edge at
/// 55% of the width.
///
/// One path for both uses — the folder clipper cuts a card with it, and
/// `FolderWidgetBorder` uses it as the header's fill — because two copies of
/// this geometry are two shapes that can disagree the day one is tuned.
Path folderOutlinePath(Size size, {double r = 20, double notch = 6}) {
  final path = Path();
  final height = size.height;
  final width = size.width;

  // Bottom left
  path.moveTo(0, height - r);
  path.quadraticBezierTo(0, height, r, height);

  // Bottom right
  path.lineTo(width - r, height);
  path.quadraticBezierTo(width, height, width, height - r);

  // Top right (notch)
  path.lineTo(width, notch + r);
  path.quadraticBezierTo(width, notch, width - r, notch);
  path.lineTo((width * 0.55) + notch, notch);
  path.lineTo((width * 0.55) + notch / 2, notch / 2);
  path.quadraticBezierTo(width * 0.55, 0, width * 0.55 - r, 0);
  path.lineTo(r, 0);
  path.quadraticBezierTo(0, 0, 0, r);
  path.close();
  return path;
}
