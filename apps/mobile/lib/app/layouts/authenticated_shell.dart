import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/broadcast/widgets/live_session_banner.dart';

/// The frame every authenticated screen sits in.
///
/// The live banner belongs here rather than inside one page: a broadcast keeps
/// running while the user moves between tabs, so the way back into it has to
/// move too. The banner draws nothing unless a session is open.
class AuthenticatedShell extends StatelessWidget {
  const new({required this.child, super.key});

  /// The signed-in content the banner sits above.
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        const LiveSessionBanner(),
        Expanded(child: child),
      ],
    );
  }
}
