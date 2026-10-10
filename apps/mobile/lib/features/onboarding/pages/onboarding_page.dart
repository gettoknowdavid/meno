import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_routing/routes.dart';
import 'package:meno_mobile/features/onboarding/manager/onboarding_manager.dart';
import 'package:meno_mobile/features/onboarding/model/onboarding_item.dart';
import 'package:meno_mobile/features/onboarding/widgets/onboarding_body.dart';
import 'package:meno_mobile/features/onboarding/widgets/onboarding_indicator.dart';
import 'package:meno_ui/meno_ui.dart';

/// Height of the wordmark at the top of the page.
const double _kLogoHeight = 32;

/// Gap between the wordmark and the carousel.
const double _kLogoGap = 72;

/// Viewport height of one slide.
const double _kCarouselHeight = 372;

/// Space kept below the actions.
const double _kBottomGap = 64;

/// The first screen of the app: a carousel of what Meno lets people do.
///
/// The carousel advances itself every five seconds and can also be swiped.
/// Either leaving action marks the intro as seen, which persists the flag
/// through [OnboardingManager] so the guard stops sending people back here.
class OnboardingPage extends WatchingWidget {
  /// Creates the intro page.
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final pageController = createOnce(PageController.new);
    final currentIndex = createOnce(() => ValueNotifier<int>(0));

    callOnce((_) {
      final timer = Timer.periodic(const Duration(seconds: 5), (_) {
        final next = currentIndex.value < onboardingItems.length - 1
            ? currentIndex.value + 1
            : 0;
        currentIndex.value = next;
        pageController.animateToPage(
          next,
          duration: const Duration(milliseconds: 250),
          curve: Curves.easeIn,
        );
      });

      onDispose(timer.cancel);
    });

    return Scaffold(
      body: SafeArea(
        child: Center(
          child: SingleChildScrollView(
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Spaces.verticalXLarge,
                if (Theme.of(context).brightness == Brightness.light)
                  Assets.images.menoPurple.image(
                    height: _kLogoHeight,
                    package: 'meno_ui',
                  )
                else
                  Assets.images.menoWhite.image(
                    height: _kLogoHeight,
                    package: 'meno_ui',
                  ),
                const SizedBox(height: _kLogoGap),
                LimitedBox(
                  maxHeight: _kCarouselHeight,
                  child: PageView.builder(
                    controller: pageController,
                    onPageChanged: (value) => currentIndex.value = value,
                    itemCount: onboardingItems.length,
                    itemBuilder: (_, i) => OnboardingBody(onboardingItems[i]),
                  ),
                ),
                Spaces.verticalXXXLarge,
                ValueListenableBuilder(
                  valueListenable: currentIndex,
                  builder: (context, value, child) => OnboardingIndicator(
                    currentIndex: value,
                    itemsLength: onboardingItems.length,
                  ),
                ),
                Spaces.verticalXLarge,
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: Insets.lg),
                  child: MPrimaryButton(
                    label: 'Get started',
                    onPressed: () => _leave(
                      context,
                      MRoutes.registerRoute(implyLeading: true),
                    ),
                  ),
                ),
                Spaces.verticalLarge,
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: Insets.lg),
                  child: MSecondaryButton(
                    label: 'Login',
                    onPressed: () =>
                        _leave(context, MRoutes.loginRoute(implyLeading: true)),
                  ),
                ),
                const SizedBox(height: _kBottomGap),
              ],
            ),
          ),
        ),
      ),
    );
  }

  /// Records the intro as seen, then pushes [location].
  ///
  /// The flag is written before navigating so the guard — which re-runs as soon
  /// as the manager notifies — never bounces the user back to the intro.
  void _leave(BuildContext context, String location) {
    di<OnboardingManager>().completeOnboarding.run();
    unawaited(context.push(location));
  }
}
