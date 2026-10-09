import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:skeletonizer/skeletonizer.dart';

/// A widget that displays a user information modal.
///
/// This modal displays the user's full name, bio (if available), and an image
/// (if provided).
/// It also provides buttons for subscribing to the user (if applicable) and
/// viewing their account.
///
/// The modal can optionally display a loading indicator while fetching user
/// information or an error message if there's an issue retrieving data.
class MUserInfoModal extends StatelessWidget {
  /// Creates a new instance of [MUserInfoModal].
  ///
  /// Parameters:
  /// - [key]: An optional key to identify the widget.
  /// - [fullName]: The user's full name. Required.
  /// - [bio]: The user's bio (optional).
  /// - [imageUrl]: The URL of the user's image (optional).
  /// - [onSubscribe]: Callback called when the user taps the subscribe button.
  /// - [onViewAccount]: Callback called when the user taps the view account
  /// button.
  /// - [loading]: Whether the modal is currently loading. Defaults to false.
  /// - [error]: Optional error message to display.
  const new({
    required this.fullName,
    super.key,
    this.bio,
    this.imageUrl,
    this.onSubscribe,
    this.onViewAccount,
    this.loading = false,
    this.error,
  });

  /// The user's full name. Required.
  final String fullName;

  /// The user's bio (optional).
  final String? bio;

  /// The URL of the user's image (optional).
  final String? imageUrl;

  /// A callback function that is called when the user taps the subscribe
  /// button.
  final VoidCallback? onSubscribe;

  /// A callback function that is called when the user taps the view account
  /// button.
  final VoidCallback? onViewAccount;

  /// Whether the modal is currently loading user information.
  final bool loading;

  /// An optional error message to display if there's an issue retrieving user
  /// data.
  final String? error;

  /// Shows the user info modal using [showDialog].
  ///
  /// Returns a [Future] that completes when the modal is closed.
  static Future<void> show({
    required BuildContext context,
    required String fullName,
    String? bio,
    String? imageUrl,
    VoidCallback? onSubscribe,
    VoidCallback? onViewAccount,
    bool loading = false,
    String? error,
  }) {
    return showDialog(
      context: context,
      builder: (context) => MUserInfoModal(
        fullName: fullName,
        bio: bio,
        imageUrl: imageUrl,
        onSubscribe: onSubscribe,
        onViewAccount: onViewAccount,
        loading: loading,
        error: error,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);

    return MModal(
      builder: (context) => Column(
        crossAxisAlignment: .stretch,
        mainAxisSize: .min,
        children: [
          if (!loading && error != null)
            Center(
              child: MText(
                error!,
                style: textTheme.bodyRegular,
                color: MColorScheme.of(context).error,
              ),
            )
          else ...[
            MAvatar(radius: 36, url: imageUrl, loading: loading),
            Spaces.verticalLarge,
            Skeletonizer(
              enabled: loading,
              child: MText(
                fullName,
                style: textTheme.heading3Medium,
                maxLines: 1,
                overflow: .ellipsis,
                textAlign: .center,
              ),
            ),
            if (bio != null) ...[
              Spaces.verticalMicro,
              Skeletonizer(
                enabled: loading,
                child: MText(
                  bio!,
                  style: textTheme.subheadingRegular,
                  maxLines: 2,
                  overflow: .ellipsis,
                  textAlign: .center,
                ),
              ),
            ],
            Spaces.verticalLarge,
            Skeletonizer(
              enabled: loading,
              child: MPrimaryButton.icon(
                label: 'Subscribe',
                icon: const Icon(MIcons.user),
                onPressed: onSubscribe,
              ),
            ),
            Spaces.verticalSmall,
            Skeletonizer(
              enabled: loading,
              child: MTextButton(
                label: 'View Account',
                onPressed: onViewAccount,
              ),
            ),
          ],
        ],
      ),
    );
  }
}
