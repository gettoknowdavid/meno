import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// A widget that displays a modal for selecting an image source.
///
/// The [MImageSourceModal] provides options to choose an image from the camera
/// or from the gallery. It takes callbacks for handling the selection of each
/// option.
///
/// Example usage:
/// ```dart
/// MImageSourceModal(
///   onCameraSourceTap: () {
///     // Handle camera source tap
///   },
///   onGallerySourceTap: () {
///     // Handle gallery source tap
///   },
/// )
/// ```
class MImageSourceModal extends StatelessWidget {
  /// Creates an instance of [MImageSourceModal].
  ///
  /// Parameters:
  /// - [key]: An optional key to identify the widget.
  /// - [onCameraSourceTap]: Callback triggered when the camera source option
  /// is tapped.
  /// - [onGallerySourceTap]: Callback triggered when the gallery source option
  /// is tapped.
  const new({
    required this.onCameraSourceTap,
    required this.onGallerySourceTap,
    super.key,
  });

  /// Callback triggered when the camera source option is tapped.
  final VoidCallback onCameraSourceTap;

  /// Callback triggered when the gallery source option is tapped.
  final VoidCallback onGallerySourceTap;

  /// Shows the image source modal using [showDialog].
  ///
  /// Returns a [Future] that completes when the modal is closed.
  static Future<void> show({
    required BuildContext context,
    required VoidCallback onCameraSourceTap,
    required VoidCallback onGallerySourceTap,
  }) {
    return showDialog(
      context: context,
      builder: (context) => MImageSourceModal(
        onCameraSourceTap: onCameraSourceTap,
        onGallerySourceTap: onGallerySourceTap,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return MModal(
      title: 'Choose Artwork',
      builder: (context) => Column(
        crossAxisAlignment: .stretch,
        mainAxisSize: .min,
        children: [
          ListTile(
            leading: Icon(MIcons.pause_circle, color: colors.onBackground),
            title: MText(
              'Choose from Camera',
              style: textTheme.bodyRegular,
              color: colors.onBackground,
            ),
            onTap: () {
              onCameraSourceTap();
              Navigator.maybePop(context);
            },
          ),
          Spaces.verticalLarge,
          ListTile(
            leading: Icon(MIcons.image, color: colors.onBackground),
            title: MText(
              'Choose from Gallery',
              style: textTheme.bodyRegular,
              color: colors.onBackground,
            ),
            onTap: () {
              onGallerySourceTap();
              Navigator.maybePop(context);
            },
          ),
        ],
      ),
    );
  }
}
