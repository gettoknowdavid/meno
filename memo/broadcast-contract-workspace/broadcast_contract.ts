// Broadcast feature: two shapes side by side, because the doc and the repo differ.
//
// This is a documentation artifact, not app code. It lives under memo/ deliberately:
// nothing in the Flutter app imports it, no build compiles it, and it is not a declared
// asset. Read it, diff it, do not run it.
//
// PART 1 — `Broadcast` and friends: what this repo's backend actually puts on the wire
//          today, read from `apps/api/src/modules/broadcast/dto.rs` and
//          `apps/api/src/infrastructure/ws/model.rs`. This is the shape the mobile DTO
//          must parse.
//
// PART 2 — `SpecBroadcast` and `SpecWsEvent`: what `docs/meno-broadcast-v2-final.md`
//          §1.2, §6.2 and §10.3 require. This is the target.
//
// PART 3 — the request DTOs both surfaces share.
//
// Diff Part 1 against Part 2 field by field; do not diff backend against mobile, because
// both are wrong in the same direction (see memo/broadcast-gap-analysis.md §2.1: the
// backend serialises camelCase and the mobile reads snake_case).
//
// Every date is RFC 3339 so a bare DateTime never serialises as time's nine-number tuple.

// ─── PART 1: the repo wire, today ────────────────────────────────────────────────

export interface Broadcast {
  readonly id: string;
  readonly title: string;
  readonly description: string | null;
  readonly timeZone: string | null;
  readonly imageUrl: string | null;
  readonly imageId: string | null;

  readonly createdAt: string;
  readonly startTime: string | null;
  readonly endTime: string | null;
  // Repo meaning: when the broadcast went live. Doc meaning: when a recording was
  // published. One column, two meanings — gap analysis D-03.
  readonly publishedAt: string | null;
  readonly durationSeconds: number | null;

  readonly status: BroadcastStatus;
  readonly state: BroadcastState;
  readonly participantRole: ParticipantRole;
  readonly isJoined: boolean;
  readonly isBookmarked: boolean;

  readonly liveParticipantsCount: number;
  readonly totalParticipants: number;

  readonly recordingEnabled: boolean;
  readonly recordingUrl: string | null;
  readonly endReason: EndReason;

  readonly timeRemainingSeconds: number | null;

  readonly creator: UserSummary;
  readonly cohosts: ReadonlyArray<UserSummary>;
}

// The session the backend hands out from go-live, join, and resume.
//
// Rust: `apps/api/src/modules/broadcast/dto.rs::BroadcastSessionResponse`
//   { broadcast: BroadcastResponse, token: String }
//
// The media URL is not in this response. The client reads the room URL from its own
// config, per docs/meno-broadcast-v2-final.md §1.2. Note the doc calls this field
// `livekit_token`; the repo calls it `token`.
export interface BroadcastSession {
  readonly broadcast: Broadcast;
  readonly token: string;
}

export interface UserSummary {
  readonly id: string;
  readonly fullName: string;
  readonly avatarId: string | null;
  readonly avatarUrl: string | null;
}

export type BroadcastStatus = 'inactive' | 'active' | 'ended';

// Doc: SCREAMING_SNAKE (`LIVE`). Repo and mobile: lowercase.
export type BroadcastState = 'live' | 'reconnecting' | 'ended' | 'scheduled' | 'draft';

// Doc: `viewer_role`, values HOST/COHOST/PARTICIPANT/NONE.
// Repo: `participant_role`, values host/cohost/participant/none.
export type ParticipantRole = 'host' | 'cohost' | 'participant' | 'none';

// Doc: Option<EndReason> with four values. Repo: non-optional, plus `none`.
export type EndReason = 'normal' | 'host_disconnected' | 'admin_forced' | 'quota_exceeded' | 'none';

// GET /broadcasts — one page. Keyset cursor, not the doc's page/limit (§5.1).
export interface BroadcastQuery {
  readonly creatorId?: string | null;
  readonly status?: BroadcastStatus | null;
  readonly order?: 'asc' | 'desc' | null;
  readonly cursor?: string | null;
  readonly limit?: number | null;
}

export interface BroadcastListItem {
  readonly id: string;
  readonly title: string;
  readonly description: string | null;
  readonly timeZone: string | null;
  readonly imageUrl: string | null;
  readonly imageId: string | null;
  readonly status: BroadcastStatus;
  readonly state: BroadcastState;
  readonly createdAt: string;
  readonly startTime: string | null;
  readonly endTime: string | null;
  readonly totalParticipants: number;
  readonly creatorId: string;
  readonly creatorName: string;
  readonly creatorAvatarId: string | null;
  readonly creatorAvatarUrl: string | null;
}

export interface CursorPage<T> {
  readonly items: ReadonlyArray<T>;
  readonly nextCursor: string | null;
  readonly hasMore: boolean;
}

// GET /broadcasts/{id}/participants — the doc splits this into /participants
// (all-time) and /live-participants (current) (§10.1).
export interface ParticipantListItem {
  readonly id: string;
  readonly fullName: string;
  readonly avatarId: string | null;
  readonly avatarUrl: string | null;
  readonly role: ParticipantRole;
  readonly joinedAt: string;
  readonly isJoined: boolean;
}

export interface ParticipantQuery {
  readonly role?: ParticipantRole | null;
  readonly order?: 'asc' | 'desc' | null;
  readonly cursor?: string | null;
  readonly limit?: number | null;
}

// POST /broadcasts/{id}/live, POST /broadcasts/{id}/join, POST /broadcasts/{id}/resume.
// The doc names the first `PUT /go-live` and does not have the third; it has
// `POST /token` for refresh instead (§10.1).
export type GoLiveResponse = BroadcastSession;
export type JoinResponse = BroadcastSession;
export type ResumeResponse = BroadcastSession;

// POST /broadcasts/{id}/leave. The doc sends this over the socket as
// `leaveBroadcast { broadcastId, positionSeconds }` (§10.2); the repo has an HTTP
// endpoint that carries no position, so continue-listening has nowhere to read from.
export interface LeaveBroadcastResponse {
  readonly success: true;
  readonly broadcastId: string;
  readonly userId: string;
  readonly leftAt: string;
}

// POST /broadcasts/{id}/end. The doc does not list an HTTP end at all (§10.2).
export interface EndBroadcastResponse {
  readonly broadcastId: string;
  readonly broadcastTitle: string;
  readonly broadcastImageUrl: string | null;
  readonly creatorId: string;
  readonly endedReason: EndReason;
  readonly endedAt: string;
  readonly durationSecs: number;
  readonly totalParticipants: number;
  readonly recordingEnabled: boolean;
}

// POST /broadcasts/{id}/cohosts and DELETE the same.
export interface CohostResponse {
  readonly user: UserSummary;
  readonly cohostCount: number;
}

// Events the backend emits today (`apps/api/src/infrastructure/ws/model.rs`).
// Names are camelCase on the wire. This list is the repo half of the yardstick;
// `SpecWsEvent` below is the doc half.
export type WsEvent =
  | { event: 'newBroadcast'; data: Broadcast }
  | { event: 'scheduledBroadcast'; data: Broadcast }
  | { event: 'broadcastDeleted'; data: { broadcastId: string } }
  | { event: 'endedBroadcast'; data: { broadcastId: string; reason: EndReason; broadcastTitle?: string; creatorId?: string } }
  | { event: 'hostDisconnected'; data: { broadcastId: string; gracePeriodInSecs: number; disconnectCount: number } }
  | { event: 'hostReconnected'; data: { broadcastId: string } }
  | { event: 'participantJoined'; data: { broadcastId: string; count: number; participant: UserSummary | null } }
  | { event: 'participantLeft'; data: { broadcastId: string; count: number; participant: UserSummary | null } }
  | { event: 'participantKicked'; data: { broadcastId: string; count: number; participant: UserSummary | null } }
  | { event: 'numberOfLiveParticipants'; data: { broadcastId: string; count: number } }
  | { event: 'cohostInvitation'; data: { broadcastId: string; token: string } }
  | { event: 'cohostAccepted'; data: { broadcastId: string; userId: string; userName: string } }
  | { event: 'cohostDeclined'; data: { broadcastId: string; userId: string } }
  | { event: 'cohostDemotion'; data: { broadcastId: string; token: string } }
  | { event: 'newCohost'; data: { broadcastId: string; livekitToken?: string } }
  | { event: 'removedCohost'; data: UserSummary }
  | { event: 'recordingReady'; data: { broadcastId: string } }
  | { event: 'recordingPublished'; data: { broadcastId: string } }
  | { event: 'notification'; data: unknown }
  | { event: 'homeInvalidated'; data: { timestamp: string } }
  | { event: 'broadcastError'; data: WsError }
  | { event: 'newMessage'; data: unknown }
  | { event: 'editedMessage'; data: unknown }
  | { event: 'deletedMessage'; data: { messageId: string; broadcastId: string } }
  | { event: 'newReaction'; data: unknown };

export interface WsError {
  readonly broadcastId: string | null;
  readonly code: WsErrorCode;
  readonly message: string;
  readonly recoverable: boolean;
  readonly data?: Record<string, unknown> | null;
}

// Repo codes (`apps/api/src/infrastructure/ws/errors.rs`) plus the doc's quota code,
// which the repo does not implement yet. `recoverable` is derived from the code.
export type WsErrorCode =
  | 'TOKEN_EXPIRED'
  | 'BROADCAST_FORCIBLY_ENDED'
  | 'KICKED_FROM_ROOM'
  | 'DAILY_QUOTA_EXCEEDED'
  | 'ROOM_NOT_FOUND'
  | 'MEDIA_SERVER_ERROR'
  | 'SERVER_SHUTDOWN'
  | 'UNSUPPORTED';

// ─── PART 2: the doc's target shape ──────────────────────────────────────────────

// `docs/meno-broadcast-v2-final.md` §6.2, translated to its wire form. Field names are
// the doc's, which serialise camelCase. Enum values are the doc's SCREAMING_SNAKE.
export interface SpecBroadcast {
  readonly id: string;
  readonly title: string;
  readonly description: string;
  readonly timeZone: string;
  readonly imageUrl: string | null;
  readonly imageId: string | null;
  readonly creatorId: string;

  readonly createdAt: string | null;
  readonly startTime: string | null;
  readonly endTime: string | null;
  readonly publishedAt: string | null;
  readonly durationSeconds: number | null;

  readonly broadcastState: SpecBroadcastState;
  readonly viewerRole: SpecViewerRole;
  readonly viewerIsInRoom: boolean;
  readonly isSubscribedToCreator: boolean;

  readonly liveParticipantsCount: number;
  readonly totalParticipants: number;

  readonly recordingEnabled: boolean;
  readonly recordingUrl: string | null;
  readonly endReason: SpecEndReason | null;

  readonly timeRemainingSeconds: number | null;
  readonly lastListenedAt: string | null;

  readonly creatorQuota: SpecQuota | null;

  readonly creator: UserSummary | null;
  readonly cohosts: ReadonlyArray<SpecCohost> | null;
}

export type SpecBroadcastState = 'LIVE' | 'RECONNECTING' | 'ENDED' | 'SCHEDULED' | 'DRAFT';
export type SpecViewerRole = 'HOST' | 'COHOST' | 'PARTICIPANT' | 'NONE';
export type SpecEndReason = 'NORMAL' | 'HOST_DISCONNECTED' | 'ADMIN_FORCED' | 'QUOTA_EXCEEDED';

export interface SpecCohost {
  readonly id: string;
  readonly fullName: string;
  readonly imageUrl: string | null;
  readonly isCohost: true;
}

// §8.1 adds `remainingDisplay`; §6.2's copy of the same type omits it (gap analysis D-07).
export interface SpecQuota {
  readonly dailyLimitSeconds: number;
  readonly usedTodaySeconds: number;
  readonly remainingSeconds: number;
  readonly remainingDisplay?: string;
  readonly resetsAt: string;
}

// §1.2, launch path. `livekitToken`, not `token`, and no URL.
export interface SpecBroadcastSession {
  readonly broadcast: SpecBroadcast;
  readonly livekitToken: string;
  // Additive later, for self-hosted scale; clients must keep the `?? config` fallback.
  readonly livekitUrl?: string | null;
}

// §4.1 keepalive answer. Only answers for a live broadcast, so the 3-value enum never
// carries ENDED in practice (gap analysis D-01).
export interface SpecKeepaliveResponse {
  readonly broadcastId: string;
  readonly connectionState: 'LIVE' | 'RECONNECTING' | 'ENDED';
  readonly liveParticipantsCount: number;
  readonly tokenExpiresAt: string;
}

// §10.2 — client → server. The repo accepts none of these today.
export type SpecClientEvent =
  | { event: 'endBroadcast'; data: { broadcastId: string } }
  | { event: 'leaveBroadcast'; data: { broadcastId: string; positionSeconds: number } }
  | { event: 'heartbeat'; data: { broadcastId: string } };

// §10.3 — server → client.
export type SpecWsEvent =
  | { event: 'newBroadcast'; data: SpecBroadcast }
  | { event: 'endedBroadcast'; data: { broadcastId: string; reason: SpecEndReason } }
  | { event: 'hostDisconnected'; data: { broadcastId: string; gracePeriodSecs: number; disconnectCount: number } }
  | { event: 'hostReconnected'; data: { broadcastId: string } }
  | { event: 'participantJoined'; data: { broadcastId: string; participant: UserSummary } }
  | { event: 'participantLeft'; data: { broadcastId: string; userId: string } }
  | { event: 'numberOfLiveParticipants'; data: { broadcastId: string; count: number } }
  | { event: 'numberOfLiveBroadcasts'; data: { count: number } }
  | { event: 'cohostInvitation'; data: { broadcastId: string; invitationId: string; inviterName: string; broadcastTitle: string } }
  | { event: 'cohostAccepted'; data: { broadcastId: string; userId: string; userName: string } }
  | { event: 'cohostDeclined'; data: { broadcastId: string; userId: string } }
  | { event: 'newCohost'; data: { broadcastId: string; livekitToken: string } }
  | { event: 'removedCohost'; data: { broadcastId: string } }
  | { event: 'recordingReady'; data: { broadcastId: string } }
  | { event: 'broadcastError'; data: SpecWsError }
  | { event: 'notification'; data: unknown };

export interface SpecWsError {
  readonly broadcastId: string;
  readonly code: SpecWsErrorCode;
  readonly message: string;
  readonly recoverable: boolean;
  readonly data?: Record<string, unknown> | null;
}

// §3.2. Note the doc has no SERVER_SHUTDOWN or UNSUPPORTED, both of which the repo emits.
export type SpecWsErrorCode =
  | 'TOKEN_EXPIRED'
  | 'BROADCAST_FORCIBLY_ENDED'
  | 'KICKED_FROM_ROOM'
  | 'DAILY_QUOTA_EXCEEDED'
  | 'ROOM_NOT_FOUND'
  | 'MEDIA_SERVER_ERROR';

// ─── PART 3: request DTOs ────────────────────────────────────────────────────────
//
// Create/update bodies are snake_case at the backend (no `rename_all` on those structs),
// which is what the mobile sends. Query parameters are camelCase. Do not "unify" these.

// POST /broadcasts.
export interface CreateBroadcastRequest {
  readonly title: string;
  readonly description?: string | null;
  readonly image_id?: string | null;
  readonly image_url?: string | null;
  readonly time_zone?: string | null;
  readonly start_time?: string | null; // RFC 3339, UTC
  readonly recording_enabled?: boolean | null;
  readonly cohosts?: ReadonlyArray<string> | null;
}

// PATCH /broadcasts/{id}.
export interface UpdateBroadcastRequest {
  readonly title?: string | null;
  readonly description?: string | null;
  readonly image_id?: string | null;
  readonly image_url?: string | null;
  readonly time_zone?: string | null;
  readonly start_time?: string | null;
  readonly recording_enabled?: boolean | null;
  readonly cohosts?: ReadonlyArray<string> | null;
}

// POST /broadcasts/{id}/cohosts.
export interface AddCohostRequest {
  readonly cohost: string;
}

// DELETE /broadcasts/{id}/cohosts/{cohostId}?removeFromRoom=true.
// The mobile sends `remove_from_room`, which the backend ignores; the default is true.
export interface RemoveCohostQuery {
  readonly removeFromRoom?: boolean;
}
