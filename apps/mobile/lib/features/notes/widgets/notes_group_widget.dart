import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/notes/manager/_manager.dart';
import 'package:meno_ui/meno_ui.dart';

class NotesGroupWidget extends StatelessWidget {
  const new({super.key, this.onTap, this.selected = false});

  final VoidCallback? onTap;
  final bool selected;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);
    final background = selected ? colors.primary : colors.inActiveContainer;
    final foreground = selected ? colors.onPrimary : colors.onInActiveContainer;

    return RawMaterialButton(
      onPressed: onTap,
      shape: RoundedSuperellipseBorder(borderRadius: .circular(20)),
      child: Container(
        height: 88,
        width: double.infinity,
        padding: const .all(16),
        decoration: BoxDecoration(
          color: background,
          borderRadius: .circular(20),
        ),
        child: Column(
          mainAxisSize: .min,
          crossAxisAlignment: .start,
          mainAxisAlignment: .spaceBetween,
          children: [
            MText('Notes', style: textTheme.captionMedium, color: foreground),
            _ValueWidget(color: foreground),
          ],
        ),
      ),
    );
  }
}

class _ValueWidget extends WatchingWidget {
  const new({required this.color});

  final Color color;

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);
    final count = watch(di<NotesManager>().totalNotesCount).value;
    return MText(
      count.toString(),
      style: textTheme.heading2Medium,
      color: color,
    );
  }
}
