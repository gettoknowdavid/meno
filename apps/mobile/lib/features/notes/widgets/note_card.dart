import 'dart:convert';

import 'package:flutter_quill/flutter_quill.dart';
import 'package:intl/intl.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/value_objects/text_value_objects.dart';
import 'package:meno_mobile/features/notes/model/_model.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:skeletonizer/skeletonizer.dart';

class NoteCard extends StatelessWidget {
  const new({
    required this.note,
    this.onTap,
    this.onOptionsTap,
    super.key,
    this.onLongPress,
    this.showAddButton = false,
    this.selected = false,
  });

  final Note note;
  final VoidCallback? onTap;
  final VoidCallback? onOptionsTap;
  final VoidCallback? onLongPress;
  final bool showAddButton;
  final bool selected;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    final content = _getContent(note.content);

    final timeStamp = note.updatedAt ?? note.createdAt ?? DateTime.now();
    final formattedDate = DateFormat('d MMM yyyy').format(timeStamp);
    final formattedTime = DateFormat('h:mm a').format(timeStamp);

    return ConstrainedBox(
      constraints: const BoxConstraints.tightForFinite(),
      child: InkWell(
        onTap: onTap,
        onLongPress: onLongPress ?? onOptionsTap,
        borderRadius: Corners.lg,
        child: Card(
          color: colors.surfaceTint,
          elevation: 0,
          shape: const RoundedRectangleBorder(borderRadius: Corners.lg),
          margin: EdgeInsets.zero,
          child: Padding(
            padding: const EdgeInsets.all(Insets.lg),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisAlignment: MainAxisAlignment.spaceBetween,
                    children: [
                      MText(
                        note.title.getOrCrash,
                        style: textTheme.bodyMedium,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
                      Spaces.verticalSmall,
                      if (note.folder != null) ...[
                        Skeleton.leaf(
                          child: Row(
                            children: [
                              MTag(
                                title: note.folder!.title.getOrCrash,
                                style: textTheme.microMedium,
                              ),
                            ],
                          ),
                        ),
                        Spaces.verticalSmall,
                      ],
                      if (content != null) ...[
                        MText(
                          content,
                          style: textTheme.captionRegular,
                          maxLines: 2,
                          overflow: TextOverflow.ellipsis,
                        ),
                        const SizedBox(height: 14),
                      ],
                      Wrap(
                        spacing: Insets.sm,
                        children: [
                          MText(
                            formattedDate,
                            style: textTheme.captionRegular,
                            color: colors.onBackgroundVariant,
                          ),
                          MText(
                            '•',
                            style: textTheme.captionRegular,
                            color: colors.onBackgroundVariant,
                          ),
                          MText(
                            formattedTime,
                            style: textTheme.captionRegular,
                            color: colors.onBackgroundVariant,
                          ),
                        ],
                      ),
                    ],
                  ),
                ),
                Spaces.horizontalSmall,
                if (showAddButton)
                  SizedBox.square(
                    dimension: Insets.lg,
                    child: Icon(
                      selected ? Icons.check_circle : MIcons.plus_circle,
                      size: 20,
                    ),
                  )
                else
                  SizedBox.square(
                    dimension: 16,
                    child: IconButton(
                      icon: const Icon(MIcons.dots_vertical),
                      padding: EdgeInsets.zero,
                      color: colors.onDisabledContainer,
                      iconSize: 20,
                      onPressed: onOptionsTap,
                    ),
                  ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

String? _getContent(MultiLineString content) {
  String? value;
  try {
    final json = content.getOrNull;
    if (json == null) {
      value = null;
    } else {
      value = Document.fromJson(jsonDecode(json) as List<dynamic>)
          .toPlainText();
    }
  } on Object {
    value = null;
  }
  return value;
}
