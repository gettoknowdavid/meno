import 'dart:async';

import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/toasts/m_toast_position.dart';
import 'package:meno_ui/src/tokens/spacing.dart';

/// Builds the content of a toast.
///
/// The second argument animates the toast away; wire it to a close button or an
/// action so the toast can be dismissed before its timer elapses.
typedef MToastBuilder = Widget Function(
  BuildContext context,
  VoidCallback dismiss,
);

/// A request to show one toast.
///
/// A request is immutable; hand it to [MToastController.show]. Prefer the
/// `BuildContext` toast extensions, which build the request for you.
final class MToastRequest {
  /// Creates a toast request.
  ///
  /// [builder] renders the content, [position] anchors it, [duration] is how
  /// long it stays on screen (pass [Duration.zero] to keep it until dismissed)
  /// and [onDismiss] runs once it has animated away.
  const new({
    required this.builder,
    this.position = .bottomCenter,
    this.duration = const Duration(seconds: 3),
    this.onDismiss,
  });

  /// Builds the toast's content.
  final MToastBuilder builder;

  /// Where the toast is anchored.
  final MToastPosition position;

  /// How long the toast stays on screen.
  final Duration duration;

  /// Called once the toast has animated away.
  final VoidCallback? onDismiss;
}

/// The single toast stack for an [OverlayState].
///
/// The controller owns one [OverlayEntry] that renders every visible toast, so
/// adding a toast never grows the overlay layer. Toasts at the same
/// [MToastPosition] stack together, and once [maxVisible] is reached new toasts
/// queue until a slot frees up.
///
/// Obtain one with [of] — never construct it directly — and drive it through
/// the `BuildContext` toast extensions.
final class MToastController extends ChangeNotifier {
  new _(this._overlay);

  /// The controllers, one per overlay, created on first use.
  static final Expando<MToastController> _controllers =
      Expando<MToastController>();

  /// The controller bound to the given overlay.
  static MToastController of(OverlayState overlay) =>
      _controllers[overlay] ??= MToastController._(overlay);

  final OverlayState _overlay;
  final List<_MToastEntry> _visible = [];
  final List<_MToastEntry> _queue = [];

  OverlayEntry? _entry;
  int _nextId = 0;

  /// The most toasts shown at once; further requests queue.
  int maxVisible = 3;

  /// The entry and exit duration of a toast.
  Duration animationDuration = const Duration(milliseconds: 260);

  /// The gap between toasts stacked at the same position.
  double spacing = Insets.sm;

  /// Queues [request] and returns the id of the new toast.
  ///
  /// The toast is shown immediately while fewer than [maxVisible] toasts are on
  /// screen, otherwise it waits its turn.
  int show(MToastRequest request) {
    final entry = _MToastEntry(_nextId++, request);
    if (_visible.length >= maxVisible) {
      _queue.add(entry);
      return entry.id;
    }
    _visible.add(entry);
    _ensureEntry();
    notifyListeners();
    return entry.id;
  }

  /// Animates the toast with [id] away.
  ///
  /// A queued toast is dropped without ever being shown.
  void dismiss(int id) {
    for (final entry in _visible) {
      if (entry.id == id) {
        unawaited(_dismiss(entry));
        return;
      }
    }
    _queue.removeWhere((entry) => entry.id == id);
  }

  /// Animates every visible toast away and drops the queue.
  Future<void> clear() async {
    _queue.clear();
    final pending = <Future<void>>[];
    for (final entry in _visible) {
      final future = _dismiss(entry);
      if (future != null) pending.add(future);
    }
    await Future.wait(pending);
  }

  /// Requests the removal of [entry], deferring until the slot is mounted.
  Future<void>? _dismiss(_MToastEntry entry) {
    if (entry.removed) return null;
    final dismiss = entry.dismiss;
    if (dismiss != null) return dismiss();
    entry.dismissRequested = true;
    return null;
  }

  void _ensureEntry() {
    if (_entry != null) return;
    _entry = OverlayEntry(builder: (_) => _MToastOverlay(controller: this));
    _overlay.insert(_entry!);
  }

  /// Finishes the removal of [entry] and promotes the next queued toast.
  void _onSlotRemoved(_MToastEntry entry) {
    if (entry.removed) return;
    entry.removed = true;
    _visible.remove(entry);
    if (_queue.isNotEmpty) _visible.add(_queue.removeAt(0));
    if (_visible.isEmpty) {
      _entry?.remove();
      _entry = null;
    }
    notifyListeners();
  }
}

/// One toast tracked by the controller.
final class _MToastEntry {
  new(this.id, this.request);

  final int id;
  final MToastRequest request;

  /// Registered by the slot once it is mounted; animates the toast away.
  Future<void> Function()? dismiss;

  /// Set when dismissal was requested before the slot mounted.
  bool dismissRequested = false;

  /// Whether the toast has been removed from the controller.
  bool removed = false;
}

/// Renders every visible toast of [controller] from a single overlay entry.
class _MToastOverlay extends StatelessWidget {
  const new({required this.controller});

  final MToastController controller;

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: controller,
      builder: (context, _) {
        final groups = <MToastPosition, List<_MToastEntry>>{};
        for (final entry in controller._visible) {
          groups.putIfAbsent(entry.request.position, () => []).add(entry);
        }
        return Stack(
          fit: .expand,
          children: [
            for (final group in groups.entries)
              _MToastGroup(
                controller: controller,
                position: group.key,
                entries: group.value,
              ),
          ],
        );
      },
    );
  }
}

/// Lays out the toasts anchored at one [position].
class _MToastGroup extends StatelessWidget {
  const new({
    required this.controller,
    required this.position,
    required this.entries,
  });

  final MToastController controller;
  final MToastPosition position;
  final List<_MToastEntry> entries;

  @override
  Widget build(BuildContext context) {
    final ordered = position.newestFirst ? entries.reversed : entries;
    return Align(
      alignment: position.alignment,
      child: SafeArea(
        child: Padding(
          padding: const .all(Insets.lg),
          child: Column(
            mainAxisSize: .min,
            spacing: controller.spacing,
            children: [
              for (final entry in ordered)
                _MToastSlot(
                  key: ValueKey(entry.id),
                  controller: controller,
                  entry: entry,
                ),
            ],
          ),
        ),
      ),
    );
  }
}

/// Animates a single toast in on mount and out before removal.
class _MToastSlot extends StatefulWidget {
  const new({required this.controller, required this.entry, super.key});

  final MToastController controller;
  final _MToastEntry entry;

  @override
  State<_MToastSlot> createState() => _MToastSlotState();
}

class _MToastSlotState extends State<_MToastSlot>
    with SingleTickerProviderStateMixin {
  late final AnimationController _animation = AnimationController(
    vsync: this,
    duration: widget.controller.animationDuration,
  );
  late final Animation<double> _progress = CurvedAnimation(
    parent: _animation,
    curve: Curves.easeOutCubic,
    reverseCurve: Curves.easeInCubic,
  );
  late final Animation<Offset> _slide = _progress.drive(
    Tween<Offset>(
      begin: widget.entry.request.position.translation,
      end: Offset.zero,
    ),
  );
  late final Animation<double> _scale = _progress.drive(
    Tween<double>(begin: 0.92, end: 1),
  );

  Timer? _timer;
  bool _dismissing = false;

  @override
  void initState() {
    super.initState();
    widget.entry.dismiss = _dismiss;
    _animation.forward();
    if (widget.entry.dismissRequested) {
      unawaited(_dismiss());
    } else {
      _scheduleAutoDismiss();
    }
  }

  void _scheduleAutoDismiss() {
    final duration = widget.entry.request.duration;
    if (duration <= Duration.zero) return;
    _timer = Timer(duration, _dismiss);
  }

  Future<void> _dismiss() async {
    if (_dismissing) return;
    _dismissing = true;
    _timer?.cancel();
    await _animation.reverse();
    if (!mounted) return;
    widget.entry.request.onDismiss?.call();
    widget.controller._onSlotRemoved(widget.entry);
  }

  @override
  void dispose() {
    _timer?.cancel();
    _animation.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final position = widget.entry.request.position;
    final content = widget.entry.request.builder(context, _dismiss);
    return FadeTransition(
      opacity: _progress,
      child: position.scales
          ? ScaleTransition(scale: _scale, child: content)
          : SlideTransition(position: _slide, child: content),
    );
  }
}
