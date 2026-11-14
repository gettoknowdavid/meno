import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/notes/model/_model.dart';
import 'package:meno_mobile/features/notes/widgets/_widgets.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:skeletonizer/skeletonizer.dart';

class FoldersListWidget extends StatelessWidget {
  const new({
    required this.folders,
    super.key,
    this.onFolderOptionsTap,
    this.onFolderTap,
    this.selectedFolder,
    this.padding = const .all(16),
    this.physics,
    this.controller,
    this.primary,
    this.isNested = false,
    this.isLoading = false,
  });

  final List<NoteFolder?> folders;
  final void Function(NoteFolder)? onFolderTap;
  final void Function(NoteFolder)? onFolderOptionsTap;
  final NoteFolder? selectedFolder;
  final ScrollPhysics? physics;
  final ScrollController? controller;
  final EdgeInsetsGeometry padding;
  final bool? primary;
  final bool isNested;
  final bool isLoading;

  @override
  Widget build(BuildContext ctx) {
    if (folders.isEmpty) {
      return const Padding(
        padding: .only(top: 158),
        child: EmptyFolderListWidget(),
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
            itemCount: folders.length,
            separatorBuilder: (_, _) => Spaces.verticalLarge,
            itemBuilder: (context, index) {
              final folder = folders[index];
              if (folder == null) return const SizedBox.shrink();

              if (isLoading) {
                return Skeletonizer(child: FolderCard(folder: folder));
              }

              return FolderCard(
                key: ValueKey(folder.id),
                folder: folder,
                onTap: () => onFolderTap?.call(folder),
                onOptionsTap: () => onFolderOptionsTap?.call(folder),
              );
            },
          ),
        ),
      ],
    );
  }
}

class FolderListFailureWidget extends StatelessWidget {
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
          'An error occurred while retrieving the folders. Please, try again?',
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
