# Running LiveKit locally

`ops/docker-compose.yml` runs a development LiveKit server alongside the other
local services. It uses LiveKit's development mode, which starts with the
placeholder key pair `devkey` / `secret`, in-memory routing and no TLS.

A development build of the mobile app talks to this server and only this server.
Starting a broadcast on a device therefore needs no internet and spends none of
the hosted project's LiveKit minutes; a release build still joins
`MENO_LIVEKIT_URL`.

## Start it

```bash
docker compose -f ops/docker-compose.yml up -d livekit
docker logs -f meno-livekit
curl -i http://localhost:7880          # 200
```

The exposed ports are:

| Port | Protocol | Purpose |
| --- | --- | --- |
| 7880 | TCP | HTTP / WebSocket signalling |
| 7881 | TCP | WebRTC media over TCP (ICE/TCP fallback) |
| 7882 | UDP | WebRTC media |

## Point a device at it

The app always uses `ws://127.0.0.1:7880` in a development build
(`Config.localLiveKitUrl`). `adb reverse` makes that address reach the server on
this machine from a phone over USB or an emulator. Rules are additive — one per
port, listed by `adb reverse --list` — so the forward the API already needs keeps
working beside these. Run the whole set after every replug: unplugging the
device or restarting the adb server clears all of them at once.

```bash
adb reverse tcp:8080 tcp:8080   # the API, unchanged
adb reverse tcp:7880 tcp:7880   # signalling
adb reverse tcp:7881 tcp:7881   # media, over TCP
```

That is all the configuration there is. The development `MENO_LIVEKIT_URL` is
ignored on purpose, so a laptop that happens to have the hosted project's URL in
`.env.dev` cannot accidentally go live against it.

`adb reverse` forwards TCP only, so media uses LiveKit's TCP fallback rather
than UDP 7882. Keep the server's default `--node-ip 127.0.0.1`. Stock Android
carries no `curl`, and a bare `nc` connect succeeds against the device end of a
forward even when nothing is listening on this machine, so check reachability
from the phone's browser — `http://127.0.0.1:7880` answers `200` — or watch the
server log for the client connecting:

```bash
docker logs -f meno-livekit
```

### The other way: this machine's LAN address

Using UDP means a phone on Wi-Fi rather than USB. Start the server with this
machine's address advertised, and change `Config.localLiveKitUrl` to it:

```bash
LIVEKIT_NODE_IP=192.168.1.42 docker compose -f ops/docker-compose.yml up -d livekit
```

```dart
static const String localLiveKitUrl = 'ws://192.168.1.42:7880';
```

Open TCP 7880/7881 and UDP 7882 in the host firewall. A phone on mobile data
cannot reach a LAN address. The constant is loopback by default because that is
the address that needs no decision: it works for an emulator, a USB device and
this machine itself.

## Where the room token comes from

A release build enters a room with the token the API minted for the session —
`POST /broadcasts/{id}/live` or `…/join`. It is a JWT the API signs with
`LIVEKIT_API_KEY` / `LIVEKIT_API_SECRET`, and it names the room after the
broadcast id.

A development build has no API-minted token today (the editor and lobby still
run on fixtures), so `DevRoomTokenSource` signs the same claims with the
development key pair. It is used only when `Config.usesLocalRoomServer` is true,
which is exactly `Environment.dev`; a release build has no signer and uses its
session token unchanged.

## Pointing the API at it

Only needed when the API is the one minting the token, e.g. when testing the
real `go live` endpoint. In `.env` (never commit this file):

```dotenv
LIVEKIT_ENABLED=true
LIVEKIT_URL=http://localhost:7880
LIVEKIT_API_KEY=devkey
LIVEKIT_API_SECRET=secret
```

The API signs participant tokens with this key pair, so it must match the
server's. Restart the API after changing them. Leaving the hosted project in
`.env` is safe for the app, which no longer reads that value in development, but
an API configured that way mints cloud tokens the local server rejects.

## Trying it with two clients

Two development clients are in the same room when they are in the same
broadcast: the dev signer names the room after the broadcast id, so a host and a
listener both land in `2a84d446-555d-4c21-acd0-a13045f23e12` while the editor is
still on fixtures. Each signs with its own account's identity, so the room sees
two participants and the host's microphone is published.

## Local image URLs

Uploaded images are served straight from the bucket, so the development bucket
is given an anonymous `s3:GetObject` policy by the compose `storage-init`
service. On a physical device, forward the storage port as well:

```bash
adb reverse tcp:9000 tcp:9000
```

`STORAGE_PUBLIC_URL` in `.env` is what is embedded in image URLs. It must be an
address the *device* can reach, not just the API process.
