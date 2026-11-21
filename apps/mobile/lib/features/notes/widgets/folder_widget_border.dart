import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/notes/widgets/folder_outline.dart';

/// The folder header's fill.
///
/// It draws no stroke: the fill is the outline. The shape is here so `Material`
/// paints and inks the header as a folder, which is why it keeps the path from
/// [folderOutlinePath] rather than a rounded rectangle.
class FolderWidgetBorder extends OutlinedBorder {
  const new({super.side});

  @override
  OutlinedBorder copyWith({BorderSide? side}) =>
      FolderWidgetBorder(side: side ?? this.side);

  @override
  Path getInnerPath(Rect rect, {TextDirection? textDirection}) =>
      folderOutlinePath(rect.size);

  @override
  Path getOuterPath(Rect rect, {TextDirection? textDirection}) =>
      folderOutlinePath(rect.size);

  @override
  void paint(Canvas canvas, Rect rect, {TextDirection? textDirection}) {
    // A stroke here would draw a second outline on the fill's own edge; the
    // shape exists for the fill and the ink, not for a border.
  }

  @override
  ShapeBorder scale(double t) => FolderWidgetBorder(side: side.scale(t));
}
