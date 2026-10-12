import 'package:flutter/material.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno/features/profile/profile.dart';
import 'package:meno/features/profile/widgets/subscribe_button.dart';
import 'package:meno_design_system/meno_design_system.dart';

class ProfileCard extends WatchingWidget {
  const ProfileCard({required this.proxy, required this.onTap, super.key});

  final UserProfileProxy proxy;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    watch(proxy);

    return InkWell(
      onTap: onTap,
      borderRadius: Corners.lg,
      child: Card(
        margin: .zero,
        shape: const RoundedRectangleBorder(borderRadius: Corners.lg),
        child: Padding(
          padding: const .all(Insets.lg),
          child: Column(
            mainAxisAlignment: .spaceBetween,
            crossAxisAlignment: .stretch,
            children: [
              MAvatar(
                radius: Insets.xxl,
                url: proxy.imageUrl,
                hasBorder: false,
              ),
              const Spacer(),
              SizedBox(
                height: Insets.xl,
                child: MText(
                  proxy.fullName,
                  style: textTheme.captionMedium,
                  maxLines: 1,
                  textAlign: .center,
                  overflow: .ellipsis,
                ),
              ),
              Spaces.verticalMicro,
              SizedBox(
                height: 18,
                child: MText(
                  proxy.stats.subscribers.toSanitizedStr('Subscriber'),
                  style: textTheme.captionRegular,
                  maxLines: 1,
                  textAlign: .center,
                  overflow: .ellipsis,
                  color: colors.onBackground.withValues(alpha: 0.6),
                ),
              ),
              const Spacer(),
              SubscribeButton(proxy: proxy),
            ],
          ),
        ),
      ),
    );
  }
}
