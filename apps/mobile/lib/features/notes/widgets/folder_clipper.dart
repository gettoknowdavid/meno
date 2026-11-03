import 'package:material_ui/material_ui.dart';

class FolderClipper extends CustomClipper<Path> {

  new({this.r = 20.0, this.notch = 6});
  final double r;
  final double notch;

  @override
  Path getClip(Size size) {
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

  @override
  bool shouldReclip(covariant CustomClipper<Path> oldClipper) => true;
}
