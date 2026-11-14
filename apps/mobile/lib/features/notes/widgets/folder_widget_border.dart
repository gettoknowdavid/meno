import 'package:material_ui/material_ui.dart';

class FolderWidgetBorder extends OutlinedBorder {
  const new({super.side});

  Path customBorderPath(Rect rect) {
    const r = 20.0;
    final path = Path();
    return path
      ..moveTo(0, rect.height - r)
      ..quadraticBezierTo(0, rect.height, r, rect.height)
      ..lineTo(rect.width - r, rect.height)
      ..quadraticBezierTo(rect.width, rect.height, rect.width, rect.height - r)
      ..lineTo(rect.width, 6 + r)
      ..quadraticBezierTo(rect.width, 6, rect.width - r, 6)
      ..lineTo((rect.width * 0.55) + 6, 6)
      ..lineTo((rect.width * 0.55) + 3, 3)
      ..quadraticBezierTo(rect.width * 0.55, 0, rect.width * 0.55 - r, 0)
      ..lineTo(r, 0)
      ..quadraticBezierTo(0, 0, 0, r)
      ..close();
  }

  @override
  OutlinedBorder copyWith({BorderSide? side}) =>
      FolderWidgetBorder(side: side ?? this.side);

  @override
  Path getInnerPath(Rect rect, {TextDirection? textDirection}) {
    return customBorderPath(rect);
  }

  @override
  Path getOuterPath(Rect rect, {TextDirection? textDirection}) {
    return customBorderPath(rect);
  }

  @override
  void paint(Canvas canvas, Rect rect, {TextDirection? textDirection}) {
    switch (side.style) {
      case BorderStyle.none:
        break;
      case BorderStyle.solid:
        canvas.drawPath(
          customBorderPath(rect),
          Paint()
            ..style = PaintingStyle.stroke
            ..color = Colors.transparent
            ..strokeWidth = 0.0,
        );
    }
  }

  @override
  ShapeBorder scale(double t) => FolderWidgetBorder(side: side.scale(t));
}
