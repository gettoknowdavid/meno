import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/notes/widgets/folder_outline.dart';

/// Clips a card to the folder outline.
class FolderClipper extends CustomClipper<Path> {
  new({this.r = 20.0, this.notch = 6});

  /// Corner radius.
  final double r;

  /// Notch depth in the top edge.
  final double notch;

  @override
  Path getClip(Size size) => folderOutlinePath(size, r: r, notch: notch);

  @override
  bool shouldReclip(covariant CustomClipper<Path> oldClipper) => true;
}
