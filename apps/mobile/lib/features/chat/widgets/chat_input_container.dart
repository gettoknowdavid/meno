import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/manager/active_broadcast_manager.dart';
import 'package:meno_mobile/features/chat/manager/chat_manager.dart';
import 'package:meno_mobile/features/chat/widgets/chat_send_button.dart';
import 'package:meno_mobile/features/chat/widgets/horizontal_popup_menu.dart';
import 'package:meno_mobile/features/chat/widgets/reactions.dart';
import 'package:meno_ui/meno_ui.dart';

/// The row a person types a message into.
///
/// The text itself is the composer's, not a `TextEditingController`'s: the
/// controller mirrors `ChatManager.content` in both directions so a switch of
/// tab — which can rebuild this field — does not lose a half-written message or
/// an edit that was started from a bubble.
class ChatInputContainer extends WatchingWidget {
  const new({required this.scrollController, super.key});

  final ScrollController scrollController;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final chat = di<ActiveBroadcastManager>().session?.chat;
    if (chat == null) return const SizedBox.shrink();

    return Container(
      alignment: .topCenter,
      padding: const .fromLTRB(16, 4, 16, 8),
      child: Column(
        children: [
          Row(
            children: [
              Expanded(child: _ChatTextField(manager: chat)),
              Spaces.horizontalLarge,
              // Reactions are a placeholder in the design: the API has the
              // frames for them, this screen has no reaction UI yet.
              HorizontalPopupMenu(
                icon: MIconButton(
                  size: 40,
                  iconSize: 20,
                  icon: const Icon(Icons.face),
                  isFilled: true,
                  fillColor: colors.outlineVariant2,
                ),
                items: mainReactions.map((reaction) {
                  return MIconButton(
                    size: 40,
                    iconSize: 20,
                    icon: reaction.icon,
                    isFilled: true,
                    fillColor: colors.outlineVariant2,
                    onPressed: () {},
                  );
                }).toList(),
              ),
              Spaces.horizontalSmall,
              ChatSendButton(manager: chat, scrollController: scrollController),
            ],
          ),
        ],
      ),
    );
  }
}

class _ChatTextField extends WatchingStatefulWidget {
  const new({required this.manager});

  final ChatManager manager;

  @override
  State<_ChatTextField> createState() => _ChatTextFieldState();
}

class _ChatTextFieldState extends State<_ChatTextField> {
  FocusNode? _focusNode;
  TextEditingController? _controller;
  ListenableSubscription? _contentSubscription;

  ChatManager get _manager => widget.manager;

  @override
  void initState() {
    super.initState();

    final initialValue = _manager.content.value.getOrElse((_) => '');
    _controller = TextEditingController(text: initialValue);

    _contentSubscription = _manager.content.listen((newValue, _) {
      _syncControllerWithManager(newValue);
    });

    _focusNode = FocusNode();
  }

  void _syncControllerWithManager(MultiLineString newValue) {
    final text = newValue.getOrElse((_) => '');
    if (_controller?.text != text) {
      _controller?.text = text;
      _controller?.selection = TextSelection.collapsed(offset: text.length);
    }
  }

  @override
  void dispose() {
    _contentSubscription?.cancel();
    _contentSubscription = null;
    _controller?.dispose();
    _controller = null;
    _focusNode?.dispose();
    _focusNode = null;
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final content = watch(_manager.content).value;

    return TextFormField(
      focusNode: _focusNode,
      style: MTextTheme.of(context).captionRegular,
      controller: _controller,
      onChanged: _manager.onContentChange,
      maxLines: 5,
      minLines: 1,
      maxLength: 244,
      keyboardType: TextInputType.multiline,
      validator: (_) => content.failureOrNull?.msg,
      decoration: const InputDecoration(
        hintText: 'Type your comment here...',
        counter: SizedBox(),
        contentPadding: .symmetric(vertical: Insets.sm, horizontal: Insets.md),
      ),
    );
  }
}
