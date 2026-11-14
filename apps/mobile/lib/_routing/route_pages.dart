/// [Page] types used by the app router to control how a destination is
/// presented.
///
/// [ModalPage] and [DialogPage] are permanent presentation helpers. [StubPage]
/// is a temporary placeholder and can be deleted once real feature pages exist.
library;

import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// Temporary placeholder page used until each feature page is implemented.
class StubPage extends StatelessWidget {
  const new(this._title, {super.key});

  final String _title;

  @override
  Widget build(BuildContext context) {
    return MScaffold(
      appBar: AppBar(title: Text(_title)),
      body: Center(child: Text(_title)),
    );
  }
}

/// Presents [child] as a modal bottom sheet route (e.g. the pre-stream lobby).
class ModalPage<T> extends Page<T> {
  const new({required this.child, super.key});

  final Widget child;

  @override
  Route<T> createRoute(BuildContext context) {
    return ModalBottomSheetRoute<T>(
      builder: (_) => Material(child: child),
      isScrollControlled: true,
      showDragHandle: true,
      useSafeArea: true,
      settings: this,
    );
  }
}

/// Presents [child] as a dismissible dialog route.
class DialogPage<T> extends Page<T> {
  const new({required this.child, this.barrierDismissible = true, super.key});

  final Widget child;
  final bool barrierDismissible;

  @override
  Route<T> createRoute(BuildContext context) {
    return DialogRoute<T>(
      context: context,
      builder: (_) => child,
      barrierDismissible: barrierDismissible,
      settings: this,
    );
  }
}
