import 'package:flutter/material.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno/features/profile/model/_model.dart';
import 'package:meno_design_system/meno_design_system.dart';
import 'package:skeletonizer/skeletonizer.dart';

class SubscribeButton extends WatchingWidget {
  const SubscribeButton({
    required this.proxy,
    this.style,
    this.showIcon = false,
    super.key,
  });

  final UserProfileProxy proxy;
  final ButtonStyle? style;
  final bool showIcon;

  @override
  Widget build(BuildContext context) {
    watch(proxy);

    final isRunning = watch(proxy.isRunning).value;

    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    final isSubscribed = proxy.isSubscribed;

    final defaultStyle = OutlinedButton.styleFrom(
      textStyle: textTheme.microMedium,
      fixedSize: const Size(double.infinity, 40),
      side: BorderSide(color: colors.primary),
      shape: const RoundedRectangleBorder(borderRadius: Corners.sm),
      backgroundColor: isSubscribed ? colors.primary : Colors.transparent,
      foregroundColor: isSubscribed ? colors.onPrimary : colors.primary,
    );

    final label = isSubscribed ? 'Unsubscribe' : 'Subscribe';

    void handleSubscription() {
      if (isSubscribed) return proxy.unsubscribe.run();
      return proxy.subscribe.run();
    }

    Widget child = MSecondaryButton(
      label: label,
      onPressed: handleSubscription,
      style: style ?? defaultStyle,
    );

    if (showIcon) {
      child = MSecondaryButton.icon(
        label: label,
        loading: isRunning,
        onPressed: isRunning ? null : handleSubscription,
        style: style ?? defaultStyle,
        icon: Icon(
          isSubscribed ? MIcons.user_minus_01 : MIcons.user_check,
          color: isSubscribed ? colors.onPrimary : colors.primary,
        ),
      );
    }

    return Skeleton.unite(child: child);
  }
}
