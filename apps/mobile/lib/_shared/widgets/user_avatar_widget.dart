import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// A presentational avatar: a name and an optional image, nothing else.
///
/// It resolves nothing from the locator and reads no credential — the caller
/// (usually `CurrentUserAvatar`, inside the account subtree) decides where the
/// data comes from, which is what keeps current-user presentation to a single
/// source (M12).
class UserAvatarWidget extends StatelessWidget {
  const new({
    required this.name,
    this.imageUrl,
    this.onTap,
    super.key,
  });

  /// The display name; used for the fallback initial.
  final String name;

  /// The image to show, or null for the initial fallback.
  final String? imageUrl;

  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final url = imageUrl;
    if (url == null || url.isEmpty) {
      return GestureDetector(
        onTap: onTap,
        child: CircleAvatar(
          radius: Insets.lg,
          child: MText(
            name.isEmpty ? '?' : name[0].toUpperCase(),
            style: MTextTheme.of(context).subheadingMedium,
          ),
        ),
      );
    }

    return MAvatar(
      radius: Insets.lg,
      url: url,
      onTap: onTap,
      hasBorder: false,
    );
  }
}
