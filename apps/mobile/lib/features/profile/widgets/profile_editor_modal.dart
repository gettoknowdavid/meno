import 'package:flutter_it/flutter_it.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/profile/manager/_manager.dart';
import 'package:meno_mobile/features/profile/model/_model.dart';
import 'package:meno_mobile/features/profile/services/_services.dart';
import 'package:meno_ui/meno_ui.dart';

/// The sheet that edits the signed-in account's profile.
///
/// The form lives in a scope pushed for this sheet, so its fields exist
/// exactly as long as it does — two visits cannot share a half-typed name —
/// and the save hands the API's answer to [MyProfileManager], which updates
/// the profile screen and the cache behind it.
class ProfileEditorModal extends WatchingWidget {
  /// Prevents instantiation: [show] is the way in.
  const new _();

  /// Shows the editor over [context].
  static Future<void> show(BuildContext context) => context.showModal<void>(
    const ProfileEditorModal._(),
    isScrollControlled: true,
    useRootNavigator: true,
  );

  @override
  Widget build(BuildContext context) {
    pushScope(
      init: (getIt) {
        getIt.registerSingleton<ProfileEditorManager>(
          ProfileEditorManager(
            profile: di<MyProfileManager>().profile.value,
            http: di<ProfileHttpService>(),
            owner: di<MyProfileManager>(),
          ),
        );
      },
    );

    // The command itself is watched rather than its results: it notifies only
    // when it finishes successfully, so there is no state to disentangle.
    registerHandler<ProfileEditorManager, MyProfile>(
      select: (manager) => manager.save,
      handler: (ctx, _, _) {
        ctx
          ..showSuccessToast('Profile updated')
          ..pop();
      },
    );

    registerHandler<ProfileEditorManager, CommandError<void>?>(
      select: (manager) => manager.save.errors,
      handler: (ctx, e, _) => ctx.showErrorToast(errorMessage(e?.error)),
    );

    final manager = di<ProfileEditorManager>();
    final isSaving = watchValue<ProfileEditorManager, bool>(
      (manager) => manager.save.isRunning,
    );
    final isValid = watch(manager.isValid).value;
    final hasChanges = watch(manager.hasChanges).value;

    return MModal(
      title: 'Edit profile details',
      builder: (context) => SingleChildScrollView(
        padding: MediaQuery.viewInsetsOf(context),
        child: Form(
          key: createOnce(GlobalKey<FormState>.new),
          autovalidateMode: .onUserInteraction,
          child: Column(
            crossAxisAlignment: .stretch,
            children: [
              Spaces.verticalLarge,
              Align(child: _Avatar(manager: manager)),
              Spaces.verticalXLarge,
              _NameField(manager: manager, enabled: !isSaving),
              Spaces.verticalXLarge,
              _BioField(manager: manager, enabled: !isSaving),
              Spaces.verticalXLarge,
              MPrimaryButton(
                label: 'Save changes',
                loading: isSaving,
                disabled: !isValid || !hasChanges,
                onPressed: manager.save.run,
              ),
              const SizedBox(height: 56),
            ],
          ),
        ),
      ),
    );
  }
}

/// The avatar, and the tap that replaces it.
class _Avatar extends StatelessWidget {
  const new({required this.manager});

  final ProfileEditorManager manager;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);

    return SizedBox.square(
      dimension: 96,
      child: Stack(
        children: [
          MAvatar(
            radius: 49,
            // A picture picked on this visit wins over the saved one.
            url: manager.currentAvatarUrl,
            file: manager.avatar.value.getFile,
            hasBorder: false,
            onTap: () => context.showModal<void>(
              MImageSourceModal(
                onGallerySourceTap: () => _pick(context, fromGallery: true),
                onCameraSourceTap: () => _pick(context, fromGallery: false),
              ),
            ),
          ),
          Align(
            alignment: .bottomRight,
            child: Container(
              height: Insets.xxl,
              width: Insets.xxl,
              decoration: BoxDecoration(
                shape: .circle,
                color: colors.primary,
                border: .all(width: 2, color: colors.onPrimary),
              ),
              child: Icon(MIcons.edit_02, size: 16, color: colors.onPrimary),
            ),
          ),
        ],
      ),
    );
  }

  /// Asks the device for a picture and hands it to the form.
  ///
  /// A refusal — a denied permission, a platform that cannot open its
  /// picker — leaves the form exactly as it was and says so: there is nothing
  /// to upload, so nothing about the save changes.
  Future<void> _pick(BuildContext context, {required bool fromGallery}) async {
    try {
      final file = await di<MediaService>().getImage(
        fromGallery: fromGallery,
      );
      if (file != null) manager.onAvatarChanged(file);
    } on Exception {
      if (context.mounted) {
        context.showErrorToast('Could not open your photos.');
      }
    }
  }
}

/// The display name field.
class _NameField extends StatelessWidget {
  const new({required this.manager, required this.enabled});

  final ProfileEditorManager manager;
  final bool enabled;

  @override
  Widget build(BuildContext context) {
    return MTextFormField(
      label: 'Name',
      hint: 'John Doe',
      required: true,
      enabled: enabled,
      initialValue: manager.fullName.value.getOrNull,
      onChanged: manager.onFullNameChanged,
      validator: (_) => manager.fullName.value.failureOrNull?.msg,
    );
  }
}

/// The biography field.
class _BioField extends StatelessWidget {
  const new({required this.manager, required this.enabled});

  final ProfileEditorManager manager;
  final bool enabled;

  @override
  Widget build(BuildContext context) {
    return MTextArea(
      label: 'Description',
      hint: 'Enter a brief description',
      maxLines: 5,
      maxLength: ProfileEditorManager.bioMax,
      enabled: enabled,
      initialValue: manager.bio.value?.getOrNull,
      onChanged: manager.onBioChanged,
      validator: (_) => manager.bio.value?.failureOrNull?.msg,
    );
  }
}
