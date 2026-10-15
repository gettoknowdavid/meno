import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_ui/meno_ui.dart';

/// What a required field says while it is empty.
///
/// The form holds plain strings now, so the message the value objects used to
/// carry is stated once here instead.
const _emptyFieldMessage = 'This value cannot be empty.';

class BroadcastEditorPage extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final manager = di<BroadcastEditorManager>();

    callOnceAfterThisBuild((context) async {
      final drafts = manager.drafts.value;
      if (drafts.isEmpty) return;

      await BroadcastDraftModal.show(
        context,
        onDraftSelected: manager.selectDraft.run,
        onCreateNew: manager.resetForm.run,
      );
    });

    // Creation is two API calls with a room in between, so the page chains
    // them: the button creates (or resumes) the row, and going live is what
    // produces the session the room is entered with. Each step reports its own
    // failure, so a chain that stops half-way leaves the form on screen.
    registerHandler<BroadcastEditorManager, void>(
      select: (manager) => manager.createBroadcast,
      handler: (context, _, _) => manager.startBroadcast.run(),
    );

    registerHandler<BroadcastEditorManager, void>(
      select: (manager) => manager.startBroadcast,
      handler: (context, _, _) {
        final broadcastId = manager.session.value?.broadcast.id;
        // An empty session means the chain stopped before the room existed, so
        // the form stays put and the commands' own handlers show why.
        if (broadcastId == null || broadcastId.isEmpty) return;

        manager.resetForm.run();
        context.go(MRoutes.liveBroadcastRoute(broadcastId.getOrCrash));
      },
    );

    final colors = MColorScheme.of(context);
    return PopScope(
      onPopInvokedWithResult: (didPop, result) {
        if (didPop) manager.resetForm.run();
      },
      child: MScaffold(
        appBar: AppBar(
          leading: const SizedBox(),
          leadingWidth: 0,
          title: const MHeader(
            title: MText.heading3('Go Live Now', weight: .bold),
            padding: .zero,
          ),
          actions: [
            InkWell(
              onTap: () {
                manager.resetForm.run();
                context.pop();
              },
              child: MText('Cancel', color: colors.onBackgroundVariant),
            ),
            Spaces.horizontalLarge,
          ],
        ),
        body: const SingleChildScrollView(child: _EditorFormWidget()),
        persistentFooterButtons: const [StartBroadcastButton()],
      ),
    );
  }
}

class _EditorFormWidget extends WatchingWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final formKey = createOnce(GlobalKey<FormState>.new);

    // Both halves of the creation: the API refusing the draft, and going live
    // failing. They are shown in place because the button that raised them is
    // on this screen.
    registerHandler<BroadcastEditorManager, CommandError<BroadcastDraft>?>(
      select: (manager) => manager.createBroadcast.errors,
      handler: (context, error, _) {
        if (error?.error == null) return;
        context.showErrorToast(errorMessage(error?.error));
      },
    );

    registerHandler<BroadcastEditorManager, CommandError<void>?>(
      select: (manager) => manager.startBroadcast.errors,
      handler: (context, error, _) {
        if (error?.error == null) return;
        context.showErrorToast(errorMessage(error?.error));
      },
    );

    final textTheme = MTextTheme.of(context);

    return Form(
      key: formKey,
      autovalidateMode: .onUserInteraction,
      child: Column(
        crossAxisAlignment: .stretch,
        children: [
          Spaces.verticalSmall,
          const _AvatarField(key: Key('broadcastForm_avatarField')),
          Spaces.verticalLarge,
          const _TitleField(key: Key('broadcastForm_titleField')),
          Spaces.verticalXLarge,
          const _DescriptionField(key: Key('broadcastForm_descriptionField')),
          Spaces.verticalXLarge,
          const CoHostSection(),
          Spaces.verticalXLarge,
          _BroadcastEditorListTile(
            leadingText: 'Remaining time today',
            subtitleText: 'Your daily broadcast time will reset in 24hrs',
            trailing: MText('0hr 30min', style: textTheme.captionRegular),
          ),
          Spaces.verticalXLarge,
          const RecordToggleSwitchField(),
          Spaces.verticalXLarge,
        ],
      ),
    );
  }
}

class _AvatarField extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);

    final manager = di<BroadcastEditorManager>();

    final isLoading = watchValue<BroadcastEditorManager, bool>(
      (m) => m.isRunning,
    );
    final imagePath = watchValue<BroadcastEditorManager, ImageInput?>(
      (m) => m.imagePath,
    );
    final imageFile = imagePath?.getFile;

    return Column(
      children: [
        MAvatar(radius: 48, file: imageFile),
        MTextButton(
          label: 'Change Artwork',
          onPressed: () {
            if (isLoading) return;
            unawaited(
              context.showModal<void>(
                MImageSourceModal(
                  onGallerySourceTap: () => unawaited(manager.pickImage()),
                  onCameraSourceTap: () =>
                      unawaited(manager.pickImage(fromGallery: false)),
                ),
              ),
            );
          },
        ),
        Center(
          child: SizedBox(
            width: 167,
            child: MText(
              'JPG or PNG accepted. Max size 10mb.',
              maxLines: 2,
              style: textTheme.microRegular,
              textAlign: TextAlign.center,
            ),
          ),
        ),
      ],
    );
  }
}

class _TitleField extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final manager = di<BroadcastEditorManager>();

    final controller = createOnce(() {
      return TextEditingController(
        text: manager.title.value.getOrElse((e) => ''),
      );
    });

    registerHandler<BroadcastEditorManager, SingleLineString>(
      select: (manager) => manager.title,
      handler: (context, title, _) {
        if (controller.text != title.getOrNull) {
          controller.text = title.getOrElse((e) => '');
        }
      },
    );

    final title = watchValue<BroadcastEditorManager, SingleLineString>(
      (m) => m.title,
    );

    final isLoading = watchValue<BroadcastEditorManager, bool>(
      (m) => m.isRunning,
    );

    return MTextFormField(
      key: key,
      label: 'Broadcast title',
      hint: "Jim Halpert's live audio",
      controller: controller,
      onChanged: manager.onTitleChanged,
      textInputAction: TextInputAction.next,
      enabled: !isLoading,
      validator: (_) =>
          title.getOrElse((e) => '').isEmpty ? _emptyFieldMessage : null,
    );
  }
}

class _DescriptionField extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final manager = di<BroadcastEditorManager>();

    final controller = createOnce(
      () => TextEditingController(
        text: manager.description.value.getOrElse((e) => ''),
      ),
    );

    registerHandler<BroadcastEditorManager, MultiLineString>(
      select: (manager) => manager.description,
      handler: (context, description, _) {
        if (controller.text != description.getOrNull) {
          controller.text = description.getOrElse((e) => '');
        }
      },
    );

    final description = watchValue<BroadcastEditorManager, MultiLineString>(
      (m) => m.description,
    );

    final isLoading = watchValue<BroadcastEditorManager, bool>(
      (m) => m.isRunning,
    );

    return MTextArea(
      key: key,
      label: 'About broadcast',
      hint: 'Enter a brief description',
      maxLines: 5,
      maxLength: 244,
      controller: controller,
      onChanged: manager.onDescriptionChanged,
      enabled: !isLoading,
      validator: (_) => description.isEmpty ? _emptyFieldMessage : null,
    );
  }
}

class CoHostSection extends StatelessWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    return const LimitedBox(
      maxHeight: 72,
      child: Row(children: [Spaces.horizontalSmall, Wrap(spacing: 8)]),
    );
  }
}

class RecordToggleSwitchField extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final manager = di<BroadcastEditorManager>();
    final record = watchValue<BroadcastEditorManager, bool>((m) => m.record);
    final isLoading = watchValue<BroadcastEditorManager, bool>(
      (m) => m.isRunning,
    );

    return _BroadcastEditorListTile(
      leadingText: 'Enable recording',
      subtitleText: 'Record your broadcast to listen back to later',
      trailing: SizedBox(
        width: 48,
        child: Switch(
          value: record,
          onChanged: isLoading ? null : manager.onToggleRecord,
        ),
      ),
    );
  }
}

class StartBroadcastButton extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final manager = di<BroadcastEditorManager>();
    final canSubmit = watchValue<BroadcastEditorManager, bool>(
      (m) => m.canSubmit,
    );
    final isLoading = watchValue<BroadcastEditorManager, bool>(
      (m) => m.isRunning,
    );
    final step = watchValue<BroadcastEditorManager, BroadcastCreationStep>(
      (m) => m.step,
    );

    final buttonLabel = switch (step) {
      .none => 'Start Broadcast',
      .created => 'Continue & Start',
      .started => 'Finalize Broadcast',
      .saved => 'Start Broadcast',
    };

    return Container(
      height: 77,
      padding: const .symmetric(horizontal: Insets.sm),
      child: Column(
        crossAxisAlignment: .stretch,
        mainAxisSize: .min,
        children: [
          MPrimaryButton(
            label: buttonLabel,
            loading: isLoading,
            disabled: isLoading || !canSubmit,
            // The draft is written before it is sent, so a creation the API
            // never answers is still on the device when the screen is reopened.
            onPressed: () async {
              final draft = await manager.saveDraft();
              manager.createBroadcast.run(draft);
            },
          ),
        ],
      ),
    );
  }
}

class _BroadcastEditorListTile extends StatelessWidget {
  const new({
    required this.leadingText,
    required this.subtitleText,
    this.trailing,
  });

  final String leadingText;
  final String subtitleText;
  final Widget? trailing;

  @override
  Widget build(BuildContext context) {
    return MInfoListTile(
      leadingText: leadingText,
      subtitleText: subtitleText,
      trailing: trailing,
    );
  }
}
