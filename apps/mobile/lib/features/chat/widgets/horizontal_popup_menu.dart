import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// A row of actions that opens above its button, in an overlay.
///
/// Deliberately not a `PopupMenuButton`: the design puts a rounded, horizontal
/// strip of icon buttons just above the field, which the Material menu cannot
/// draw.
class HorizontalPopupMenu extends StatefulWidget {
  const new({required this.items, required this.icon, super.key});

  final List<Widget> items;
  final Widget icon;

  @override
  State<HorizontalPopupMenu> createState() => _HorizontalPopupMenuState();
}

class _HorizontalPopupMenuState extends State<HorizontalPopupMenu> {
  OverlayEntry? _overlayEntry;
  bool _isMenuOpen = false;

  @override
  Widget build(BuildContext context) {
    return GestureDetector(onTap: _showPopupMenu, child: widget.icon);
  }

  @override
  void dispose() {
    _overlayEntry?.remove();
    _overlayEntry = null;
    super.dispose();
  }

  void _closePopupMenu() {
    _overlayEntry?.remove();
    _overlayEntry = null;
    _isMenuOpen = false;
  }

  void _showPopupMenu() {
    if (_isMenuOpen) {
      _closePopupMenu();
      return;
    }

    final button = context.findRenderObject()! as RenderBox;
    final overlay =
        Navigator.of(
              context,
              rootNavigator: true,
            ).overlay!.context.findRenderObject()!
            as RenderBox;

    // Above the button, and inset from the right edge by the room's padding.
    final buttonPosition = button.localToGlobal(Offset.zero, ancestor: overlay);
    final popupTop = buttonPosition.dy - 160 - 8;
    final popupLeft =
        overlay.size.width - buttonPosition.dx - button.size.width - 16;

    _overlayEntry = OverlayEntry(
      builder: (context) => GestureDetector(
        onTap: _closePopupMenu,
        behavior: HitTestBehavior.translucent,
        child: Stack(
          children: [
            Positioned.fill(child: Container(color: Colors.transparent)),
            Positioned(
              top: popupTop,
              right: popupLeft,
              child: Material(
                elevation: 4,
                borderRadius: Corners.circle,
                child: Container(
                  height: 56,
                  padding: const EdgeInsets.all(Insets.sm),
                  decoration: BoxDecoration(
                    color: MColorScheme.of(context).background,
                    borderRadius: Corners.circle,
                  ),
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    spacing: 4,
                    children: widget.items,
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );

    Overlay.of(context).insert(_overlayEntry!);
    _isMenuOpen = true;
  }
}
