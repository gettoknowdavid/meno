import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/notes/manager/_manager.dart';
import 'package:meno_ui/meno_ui.dart';

class FoldersGroupWidget extends StatelessWidget {
  const new({super.key, this.onTap, this.selected = false});

  final VoidCallback? onTap;
  final bool selected;

  @override
  Widget build(BuildContext context) {
    final size = MediaQuery.sizeOf(context);
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    final background = selected ? colors.primary : colors.inActiveContainer;
    final foreground = selected ? colors.onPrimary : colors.onInActiveContainer;

    return RawMaterialButton(
      onPressed: onTap,
      shape: const FoldersGroupWidgetBorder(),
      fillColor: background,
      elevation: 0,
      hoverElevation: 0,
      focusElevation: 0,
      highlightElevation: 0,
      child: Container(
        height: 94,
        width: size.width,
        padding: const .fromLTRB(16, 22, 16, 16),
        child: Column(
          mainAxisSize: .min,
          crossAxisAlignment: .start,
          mainAxisAlignment: .spaceBetween,
          children: [
            MText('Folders', style: textTheme.captionMedium, color: foreground),
            _ValueWidget(color: foreground),
          ],
        ),
      ),
    );
  }
}

class _ValueWidget extends WatchingWidget {
  const new({required this.color});

  final Color color;

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);
    final count = watch(di<FoldersManager>().totalFoldersCount).value;
    return MText(
      count.toString(),
      style: textTheme.heading2Medium,
      color: color,
    );
  }
}

class FoldersGroupWidgetBorder extends OutlinedBorder {
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
      FoldersGroupWidgetBorder(side: side ?? this.side);

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
  ShapeBorder scale(double t) => FoldersGroupWidgetBorder(side: side.scale(t));
}
