import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/locator.dart';
import 'package:meno_ui/meno_ui.dart';

class MenoApp extends StatelessWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp.router(
      debugShowCheckedModeBanner: false,
      theme: MTheme.light,
      darkTheme: MTheme.dark,
      routerConfig: di<GoRouter>(),
    );
  }
}
