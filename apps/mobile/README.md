# meno_mobile

The Flutter client for Meno.

## Configuration

The API, socket and LiveKit URLs are compiled in from `.env.dev` and
`.env.prod` by `envied` (`lib/_core/config/env.dart`). Neither file is
committed; `.env.example` names the three keys:

```
MENO_API_URL=
WEB_SOCKET_URL=
MENO_LIVEKIT_URL=
```

After creating or editing one, regenerate the compiled values:

```sh
dart run build_runner build --delete-conflicting-outputs
```

A blank URL is not a compile error: `Config.validate` fails at startup and the
app shows the configuration error screen instead of running half-configured.

## Run

```sh
flutter run -t lib/main_dev.dart   # dev entry point, .env.dev values
flutter run                        # lib/main.dart, .env.prod values
```

## Check

```sh
flutter analyze
flutter test
```
