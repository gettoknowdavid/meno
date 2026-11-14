import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/value_objects/id.dart';
import 'package:meno_mobile/features/notes/model/_model.dart';
import 'package:meno_mobile/features/notes/widgets/_widgets.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:skeletonizer/skeletonizer.dart';

class NotesListWidget extends StatelessWidget {
  const new({
    required this.notes,
    this.showAddButton = false,
    this.onNoteTap,
    this.onNoteLongPress,
    this.onNoteOptionsTap,
    this.selectedNoteIds = const [],
    this.padding = const .all(16),
    this.physics,
    this.controller,
    this.primary,
    this.isNested = false,
    this.isLoading = false,
    super.key,
  });

  final List<Note?> notes;
  final bool showAddButton;
  final void Function(Note)? onNoteTap;
  final void Function(Note)? onNoteLongPress;
  final void Function(Note)? onNoteOptionsTap;
  final List<Id> selectedNoteIds;
  final EdgeInsetsGeometry padding;
  final ScrollPhysics? physics;
  final ScrollController? controller;
  final bool? primary;
  final bool isNested;
  final bool isLoading;

  @override
  Widget build(BuildContext ctx) {
    if (notes.isEmpty) {
      return const Padding(
        padding: .only(top: 158),
        child: EmptyNoteListWidget(),
      );
    }
    return CustomScrollView(
      primary: primary,
      controller: controller,
      physics: physics,
      slivers: [
        if (isNested) ...[
          Builder(
            builder: (context) => SliverOverlapInjector(
              handle: NestedScrollView.sliverOverlapAbsorberHandleFor(context),
            ),
          ),
        ],
        SliverPadding(
          padding: padding,
          sliver: SliverList.separated(
            itemCount: notes.length,
            separatorBuilder: (_, _) => Spaces.verticalLarge,
            itemBuilder: (context, index) {
              final note = notes[index];
              if (note == null) return const SizedBox.shrink();

              if (isLoading) return Skeletonizer(child: NoteCard(note: note));

              return NoteCard(
                key: ValueKey(note.id),
                note: note,
                showAddButton: showAddButton,
                selected: selectedNoteIds.contains(note.id),
                onTap: () => onNoteTap?.call(note),
                onLongPress: () => onNoteLongPress?.call(note),
                onOptionsTap: () => onNoteOptionsTap?.call(note),
              );
            },
          ),
        ),
      ],
    );
  }
}

class NoteListFailureWidget extends StatelessWidget {
  const new({required this.onRefresh, super.key});

  final Future<void> Function() onRefresh;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);
    return Column(
      children: [
        const SizedBox(height: 72),
        MText(
          'An error occurred while retrieving the notes. Please, try again?',
          style: textTheme.bodyRegular,
          textAlign: TextAlign.center,
        ),
        Spaces.verticalXLarge,
        SizedBox(
          height: 32,
          child: MSecondaryButton.icon(
            label: 'Reload',
            icon: const Icon(Icons.refresh),
            style: OutlinedButton.styleFrom(
              textStyle: textTheme.microMedium,
              foregroundColor: colors.onBackground,
              iconColor: colors.onBackground,
              shape: const RoundedRectangleBorder(borderRadius: Corners.sm),
              side: BorderSide(color: colors.outlineVariant3, width: 1.50),
            ),
            onPressed: onRefresh,
          ),
        ),
      ],
    );
  }
}
