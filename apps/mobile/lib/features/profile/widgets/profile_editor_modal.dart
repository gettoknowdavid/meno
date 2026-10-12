import 'package:flutter/material.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno/_core/_core.dart';
import 'package:meno/_routing/_routing.dart';
import 'package:meno/_shared/_shared.dart';
import 'package:meno/features/profile/model/_model.dart';
import 'package:meno_design_system/meno_design_system.dart';

class ProfileEditorModal extends WatchingWidget {
  const ProfileEditorModal._(this.profile) : super(key: null);
  final Profile profile;

  static Future<dynamic> show(BuildContext context, Profile profile) {
    return showModalBottomSheet<dynamic>(
      context: context,
      builder: (context) => ProfileEditorModal._(profile),
      isScrollControlled: true,
      useRootNavigator: true,
    );
  }

  @override
  Widget build(BuildContext context) {
    final proxy = createOnce(() => MyProfileProxy(profile));
    final formKey = createOnce(GlobalKey<FormState>.new);

    registerHandler(
      target: proxy.updateProfile.results,
      handler: (context, CommandResult<void, void> results, cancel) {
        if (results.isSuccess) return context.pop();
        if (results.hasError) {
          final error = results.error;
          if (error == null) return;
          final message = switch (error) {
            MenoException(:final message) => message,
            _ => error.toString(),
          };
          context.showErrorSnackBar(message);
        }
      },
    );

    watch(proxy);

    return MModal(
      title: 'Edit profile details',
      builder: (context) => SingleChildScrollView(
        padding: MediaQuery.viewInsetsOf(context),
        child: Form(
          key: formKey,
          autovalidateMode: .onUserInteraction,
          child: Column(
            crossAxisAlignment: .stretch,
            children: [
              Spaces.verticalLarge,
              Align(child: _Avatar(proxy: proxy)),
              Spaces.verticalXLarge,
              _NameField(proxy: proxy),
              Spaces.verticalXLarge,
              _BioField(proxy: proxy),
              Spaces.verticalXLarge,
              _SubmitButton(proxy: proxy),
              const SizedBox(height: 56),
            ],
          ),
        ),
      ),
    );
  }
}

class _Avatar extends WatchingWidget {
  const _Avatar({required this.proxy});

  final MyProfileProxy proxy;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    return SizedBox.square(
      dimension: 96,
      child: Stack(
        children: [
          MAvatar(
            radius: 49,
            url: proxy.profile.image?.getUrl(),
            file: proxy.image?.getFile(),
            hasBorder: false,
            onTap: () => context.showModal<void>(
              MImageSourceModal(
                onGallerySourceTap: proxy.onImageChanged,
                onCameraSourceTap: () => proxy.onImageChanged(false),
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
}

class _NameField extends WatchingWidget {
  const _NameField({required this.proxy});

  final MyProfileProxy proxy;

  @override
  Widget build(BuildContext context) {
    final isUpdating = watch(proxy.updateProfile.isRunning).value;
    return MTextFormField(
      label: 'Name',
      hint: 'John Doe',
      required: true,
      enabled: !isUpdating,
      initialValue: proxy.fullName.getOrNull(),
      onChanged: proxy.onNameChanged,
      validator: (_) => proxy.fullName.failureOrNull?.msg,
    );
  }
}

class _BioField extends WatchingWidget {
  const _BioField({required this.proxy});

  final MyProfileProxy proxy;

  @override
  Widget build(BuildContext context) {
    final isUpdating = watch(proxy.updateProfile.isRunning).value;
    return MTextArea(
      label: 'Description',
      hint: 'Enter a brief description',
      maxLines: 5,
      maxLength: 244,
      enabled: !isUpdating,
      initialValue: proxy.bio?.getOrNull(),
      onChanged: proxy.onBioChanged,
      validator: (_) => proxy.bio?.failureOrNull?.msg,
    );
  }
}

class _SubmitButton extends WatchingWidget {
  const _SubmitButton({required this.proxy});

  final MyProfileProxy proxy;

  @override
  Widget build(BuildContext context) {
    final isUpdating = watch(proxy.updateProfile.isRunning).value;
    final isFormValid = watch(proxy.isFormValid).value;
    final hasChanges = watch(proxy.hasChanges).value;

    return MPrimaryButton(
      label: 'Save changes',
      loading: isUpdating,
      disabled: !isFormValid || !hasChanges,
      onPressed: proxy.updateProfile.run,
    );
  }
}
