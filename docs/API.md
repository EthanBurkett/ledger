# Ledger HTTP API Reference

Everything a client (web, mobile, CLI, CI bot) needs to talk to a running
`ledger` server. Treat this document as the source of truth; every route,
field, and error code below is implemented in `src/api/` and
`src/core/`.

---

## Table of contents

1. [Overview](#1-overview)
2. [Transport](#2-transport)
3. [Response envelope](#3-response-envelope)
4. [Errors](#4-errors)
5. [Authentication](#5-authentication)
6. [Endpoints — Health](#6-endpoints--health)
7. [Endpoints — Auth](#7-endpoints--auth)
8. [Endpoints — Repos](#8-endpoints--repos)
9. [Endpoints — Refs](#9-endpoints--refs)
10. [Endpoints — Blobs](#10-endpoints--blobs)
11. [Endpoints — Trees](#11-endpoints--trees)
12. [Endpoints — Commits](#12-endpoints--commits)
13. [Endpoints — Staging index](#13-endpoints--staging-index)
14. [Endpoints — Diff](#14-endpoints--diff)
15. [Type glossary](#15-type-glossary)
16. [HTTP status codes](#16-http-status-codes)
17. [Error code catalog](#17-error-code-catalog)
18. [Workflows](#18-workflows)
19. [Implementation notes](#19-implementation-notes)

---

## 1. Overview

Ledger is a git-style, content-addressed object store backed by MongoDB.
The HTTP API exposes eight resources:

| Resource | Purpose |
| --- | --- |
| `auth` | Account creation, login/logout, token refresh |
| `repos` | Top-level containers owned by a user |
| `refs` | Named pointers (branches/tags) per repo |
| `blobs` | Immutable binary content, identified by `sha256(content)` |
| `trees` | Directory snapshots, identified by the hash of their canonical form |
| `commits` | Snapshots of a repo at a point in time, each referencing one tree + zero or more parent commits |
| `index` | Per-repo staging area, turned into a commit via one call |
| `diff` | Change sets between two trees or two commits |

Key guarantees:

- **Content addressing.** Blob, tree, and commit IDs are deterministic
  hashes of their content. Uploading the same bytes twice collapses to
  the same object.
- **Owner scoping.** All repo operations are scoped to the authenticated
  user. A repo you don't own is indistinguishable from one that doesn't
  exist (both return `404`).
- **Deterministic envelopes.** Every response has the same outer shape —
  success or error — so clients can use a single wrapper.

---

## 2. Transport

### Base URL

Defaults to `http://127.0.0.1:3030`. The server binds wherever
`--http-addr` (or `LEDGER_HTTP_ADDR`) points. In production put a TLS
terminator in front of it.

### Content type

- **Requests:** `Content-Type: application/json; charset=utf-8`.
- **Responses:** `Content-Type: application/json; charset=utf-8`.

Binary data (blob content) is **base64-encoded JSON**, not raw bytes.

### API version

A single major version is in flight at any time. The current prefix is
`/v1/...`. The version is also echoed in every response envelope
(`meta.api_version`), so clients can assert it.

### Request IDs

Every response carries an `x-request-id` header and a matching
`meta.request_id` in the envelope. Clients may set their own
`x-request-id: <string>` header on the request; if present (≤128 chars,
non-empty) the server reuses it end-to-end. Otherwise the server
generates a UUIDv7.

Use this ID in logs and bug reports — it is the single identifier that
ties the request, handler, database work, and error envelope together.

### CORS

The server ships with a real CORS layer (`tower_http::cors`) wired in
`src/api/mod.rs`. It always:

- handles `OPTIONS` preflight requests internally (so you won't see a
  `405` on a preflight);
- allows the methods the API uses (`GET POST PUT PATCH DELETE OPTIONS`);
- allows request headers the frontend needs (`content-type`,
  `authorization`, `accept`, `x-request-id`);
- exposes `x-request-id` back to JavaScript so clients can read the
  request ID from the response;
- caches preflight responses for 10 minutes.

Origin policy is controlled by the `LEDGER_CORS_ORIGIN` environment
variable on the server:

| Value | Behavior |
| --- | --- |
| unset or `*` *(default)* | Accept any origin. Credentials mode **off** (this is correct — Ledger authenticates via `Authorization: Bearer`, not cookies). |
| `https://app.example.com` | Only that exact origin. Credentials mode on. |
| `https://a.example.com,https://b.example.com` | Comma-separated list of exact origins. Credentials mode on. |

Notes for the frontend:

- Send the `Authorization` header explicitly on every request. With
  `fetch`, that means passing `headers: { Authorization: "Bearer …" }`;
  **do not** set `credentials: "include"` unless you've configured
  `LEDGER_CORS_ORIGIN` to an explicit origin list (wildcard + credentials
  is forbidden by the CORS spec).
- If you ever see a `405` or a "CORS missing" error, the usual culprits
  are: (a) you're hitting a route that doesn't exist (the preflight
  still works, but the actual request then fails differently), or
  (b) the server was started with a `LEDGER_CORS_ORIGIN` value that
  doesn't include your frontend origin. Check server logs for the
  `cors:` line emitted at startup — it tells you which mode is active.

---

## 3. Response envelope

Every response, success *and* error, is wrapped in an `Envelope`:

```jsonc
{
  "meta": {
    "request_id": "0194e013-5e11-7b94-a88b-cd3ae1a1b28c",
    "timestamp":  "2026-04-18T02:30:07.412Z",   // RFC3339 millisecond precision, UTC
    "api_version": "v1",
    "duration_ms": 4
  },

  // Exactly one of `data` or `errors` is present.
  "data":   { /* the response body, shape depends on endpoint */ },
  "errors": [ /* array of ApiErrorObject, see §4 */ ]
}
```

Rules:

- `meta` is always present.
- On success, `data` is set and `errors` is absent (or `null`).
- On failure, `errors` is set (length ≥ 1) and `data` is absent.
- `data` may be an object, array, or primitive value depending on the
  endpoint (endpoint docs below show exactly what to expect).

### TypeScript wrapper

```ts
type Meta = {
  request_id: string;
  timestamp: string;           // ISO 8601, UTC
  api_version: "v1";
  duration_ms: number;
};

type ApiErrorObject = {
  status: number;              // HTTP status
  code: string;                // machine-readable, see §17
  title: string;               // human title
  detail?: string;             // human explanation
  source?: unknown;            // optional contextual object (JSON pointer, parameter name, etc.)
};

type Envelope<T> = {
  meta: Meta;
  data?: T;
  errors?: ApiErrorObject[];
};
```

---

## 4. Errors

### Shape

```json
{
  "meta": { ... },
  "errors": [
    {
      "status": 422,
      "code": "validation_failed",
      "title": "Request validation failed",
      "detail": "username must be 3-32 characters",
      "source": { "pointer": "/username" }
    }
  ]
}
```

Notes:

- `errors` is always an array, but the current handlers emit exactly one
  entry. Front-ends should still iterate defensively.
- `status` matches the HTTP response status.
- `code` is **stable** — use it for branching logic (never parse
  `detail`).
- `source` is a free-form JSON object. For validation errors it is
  commonly `{ "pointer": "/<field>" }`; for parameter errors it is
  commonly `{ "parameter": "<name>" }`; it may also be `null`/absent.

The full code catalog is in [§17](#17-error-code-catalog).

### Common failure modes worth wiring up

| Situation | HTTP | `code` |
| --- | --- | --- |
| No `Authorization` header on a protected route | 401 | `missing_token` |
| `Authorization` not `Bearer` | 401 | `invalid_scheme` |
| Token malformed / wrong signature | 401 | `invalid_token` |
| Token expired | 401 | `token_expired` |
| Wrong token type (refresh used where access expected, etc.) | 401 | `wrong_token_type` |
| Bad username/password on `/auth/login` | 401 | `invalid_credentials` |
| Refresh token used twice → all sessions killed | 401 | `token_reuse_detected` |
| JSON body failed to parse | 400 | `invalid_json` |
| Required field missing or shape bad | 422 | `validation_failed` |
| Unique violation (repo name taken, ref exists, …) | 409 | `conflict` |
| Missing resource or resource the caller doesn't own | 404 | `not_found` |
| Unexpected server failure | 500 | `internal_error` / `database_error` |

---

## 5. Authentication

### Model

- **JWT access token** — short-lived (15 min default). Passed as
  `Authorization: Bearer <token>` on every non-auth route.
- **JWT refresh token** — long-lived (7 days default). Only accepted at
  `POST /v1/auth/refresh`. Stored server-side; revocation and reuse
  detection are real.
- **No cookies.** Ledger never sets a cookie. Clients persist tokens
  themselves (for web: in memory + `sessionStorage` is recommended;
  avoid `localStorage` for access tokens).
- **Stay logged in.** On login the caller may set `stay_logged_in: true`
  to request tokens with a longer TTL. The default multiplier is `4×`
  (configured by `AuthConfig::stay_logged_in_multiplier`), so a normal
  session is 15 min access / 7 day refresh, and a long session is 1 hr
  access / 28 day refresh.

### Token flow

```
register / login
       │
       ▼
 access_token + refresh_token (client stores both)
       │
       ▼  (every request)
Authorization: Bearer <access_token>
       │
       ▼  (on 401 with access token)
POST /v1/auth/refresh { refresh_token }
       │
       ▼
new access_token + new refresh_token
(old refresh_token is revoked; reusing it
 forcibly revokes every outstanding session)
```

### Claims

The access and refresh tokens share the same JWT payload schema:

```ts
type Claims = {
  sub: string;     // ObjectId hex of the user
  exp: number;     // unix seconds
  iat: number;     // unix seconds
  iss: string;     // "ledger" (configurable)
  jti: string;     // UUID v4 — refresh tokens are pinned to DB records by jti
  typ: "access" | "refresh";
  sli: boolean;    // stay_logged_in flag (client hint only; server trusts its own config)
};
```

Frontends should treat these as opaque. Don't decode and trust `exp`
for UI state because clocks drift — instead use the `access_expires_in`
returned with the pair as a *soft* hint, and always let the server
reject with `401` + `token_expired` as the authoritative signal.

### Required header

On every non-`/v1/auth/(register|login|refresh|logout)` route:

```
Authorization: Bearer eyJhbGciOi…
```

The request-id header is also supported but always optional:

```
x-request-id: my-client-generated-uuid-v7
```

---

## 6. Endpoints — Health

### `GET /v1/health`

Liveness probe. Does not require auth.

**Response 200**

```json
{
  "meta": { ... },
  "data": {
    "status":  "ok",
    "service": "ledger",
    "version": "0.1.0"
  }
}
```

`version` is the Cargo package version of the running server.

---

## 7. Endpoints — Auth

All paths: `/v1/auth/...`. No bearer token required unless noted.

### `POST /v1/auth/register`

Creates a user **and** immediately logs them in (you get tokens back).
Use this for first-time signup.

**Request body**

```json
{
  "username": "alice",
  "password": "correct-horse-battery-staple"
}
```

| Field | Type | Rules |
| --- | --- | --- |
| `username` | string | 3–32 chars, ASCII `[A-Za-z0-9_.-]` |
| `password` | string | 8–256 chars |

**Response 201 — Session**

```json
{
  "meta": { ... },
  "data": {
    "user": {
      "id": "66211af49a4d3a001deadbee0",
      "username": "alice",
      "created_at": 1744934000,
      "last_login_at": 1744934001
    },
    "tokens": {
      "access_token":       "eyJhbGciOi…",
      "refresh_token":      "eyJhbGciOi…",
      "token_type":         "Bearer",
      "access_expires_in":  900,
      "refresh_expires_in": 604800,
      "stay_logged_in":     false
    }
  }
}
```

**Errors**

| HTTP | Code | Meaning |
| --- | --- | --- |
| 409 | `conflict` | Username already taken |
| 422 | `validation_failed` | Username/password failed validation rules |

---

### `POST /v1/auth/login`

Exchange credentials for tokens.

**Request body**

```json
{
  "username":       "alice",
  "password":       "…",
  "stay_logged_in": false
}
```

`stay_logged_in` is optional (default `false`).

**Response 200 — Session** (same shape as `register`)

**Errors**

| HTTP | Code | Meaning |
| --- | --- | --- |
| 401 | `invalid_credentials` | Wrong username or password |

### `POST /v1/auth/refresh`

Rotate a refresh token. Each successful call **invalidates** the
presented refresh token; a reuse attempt revokes every outstanding
session for that user (probable theft).

**Request body**

```json
{
  "refresh_token": "eyJhbGciOi…"
}
```

**Response 200 — TokenPair**

```json
{
  "meta": { ... },
  "data": {
    "access_token":       "eyJhbGciOi…",
    "refresh_token":      "eyJhbGciOi…",
    "token_type":         "Bearer",
    "access_expires_in":  900,
    "refresh_expires_in": 604800,
    "stay_logged_in":     false
  }
}
```

**Errors**

| HTTP | Code | Meaning |
| --- | --- | --- |
| 401 | `invalid_token` | Malformed or bad-signature refresh token |
| 401 | `token_expired` | Refresh token past `exp` |
| 401 | `wrong_token_type` | Access token presented instead of refresh |
| 401 | `token_reuse_detected` | Refresh token was already used; every other refresh for this user has just been revoked. Re-login. |

### `POST /v1/auth/logout`

Revokes a refresh token server-side. Idempotent — unknown/expired
tokens are silently accepted so clients can "always try".

**Request body**

```json
{
  "refresh_token": "eyJhbGciOi…"
}
```

**Response 200**

```json
{ "meta": { ... }, "data": { "logged_out": true } }
```

### `GET /v1/auth/me`

Returns the currently authenticated user. Requires bearer token.

**Response 200 — UserView**

```json
{
  "meta": { ... },
  "data": {
    "id": "66211af49a4d3a001deadbee0",
    "username": "alice",
    "created_at": 1744934000,
    "last_login_at": 1744934001
  }
}
```

---

## 8. Endpoints — Repos

All paths: `/v1/repos[/...]`. All routes require bearer auth. Every
route is **owner-scoped**: you only see your own repos, and accessing a
repo owned by somebody else returns `404 not_found` (same as if it
didn't exist — deliberate, to hide other users).

### `GET /v1/repos`

List your repos.

**Response 200** — `RepoView[]`

```json
{
  "meta": { ... },
  "data": [
    {
      "id":          "66211bf49a4d3a001deadb111",
      "owner_id":    "66211af49a4d3a001deadbee0",
      "name":        "main-repo",
      "head_commit": "a14c1d…"
    }
  ]
}
```

### `POST /v1/repos`

Create a repo owned by the authenticated user. Repo names are globally
unique across all users (currently enforced by a unique index on
`repos.name`).

**Request body**

```json
{ "name": "main-repo" }
```

| Field | Type | Rules |
| --- | --- | --- |
| `name` | string | 1–128 chars, `[A-Za-z0-9_.-/]` |

**Response 201 — RepoView**

**Errors**

| HTTP | Code | Meaning |
| --- | --- | --- |
| 409 | `conflict` | Name already exists |
| 422 | `validation_failed` | Name is empty, too long, or contains disallowed chars |

### `GET /v1/repos/{id}`

Fetch a single repo by `ObjectId` hex.

**Response 200 — RepoView**

**Errors**

| HTTP | Code | Meaning |
| --- | --- | --- |
| 400 | `bad_request` | `id` is not a valid 24-char hex ObjectId |
| 404 | `not_found` | No such repo, or the caller doesn't own it |

### `DELETE /v1/repos/{id}`

Delete the repo document. Does **not** cascade-delete its refs, commits,
trees, or blobs — those stay behind so content can be inspected or
garbage-collected later.

**Response 200**

```json
{ "meta": { ... }, "data": { "deleted": true, "id": "66211bf49a4d3a001deadb111" } }
```

### `PUT /v1/repos/{id}/head`

Advance HEAD to a specific commit hash.

**Request body**

```json
{ "commit": "a14c1d…" }
```

**Response 200 — RepoView** (with `head_commit` reflecting the new value)

### `DELETE /v1/repos/{id}/head`

Clear HEAD (sets `head_commit` to `null`).

**Response 200 — RepoView**

---

## 9. Endpoints — Refs

Named pointers (think git branches/tags) scoped per repo. All routes
require bearer auth and verify repo ownership first.

### `GET /v1/repos/{id}/refs`

List refs for the repo.

**Response 200** — `RefView[]`

```json
{
  "meta": { ... },
  "data": [
    {
      "id":       "66211cf49a4d3a001deadb222",
      "repo_id":  "66211bf49a4d3a001deadb111",
      "name":     "main",
      "commit":   "a14c1d…"
    }
  ]
}
```

### `POST /v1/repos/{id}/refs`

Create a new ref.

**Request body**

```json
{ "name": "main", "commit": "a14c1d…" }
```

| Field | Type | Rules |
| --- | --- | --- |
| `name` | string | 1–128 chars, `[A-Za-z0-9_.-/]` |
| `commit` | string | Commit hash, non-empty |

Uniqueness is enforced on `(repo_id, name)`.

**Response 201 — RefView**

**Errors**

| HTTP | Code | Meaning |
| --- | --- | --- |
| 409 | `conflict` | A ref with that name already exists in this repo |

### `GET /v1/repos/{id}/refs/{name}`

**Response 200 — RefView**

### `PATCH /v1/repos/{id}/refs/{name}`

Move a ref to another commit.

**Request body**

```json
{ "commit": "b22f48…" }
```

**Response 200 — RefView** (with `commit` reflecting the new value)

### `DELETE /v1/repos/{id}/refs/{name}`

**Response 200**

```json
{ "meta": { ... }, "data": { "deleted": true, "name": "main" } }
```

---

## 10. Endpoints — Blobs

Blobs are raw bytes, identified by `sha256(content)` (lower-case hex,
64 chars). All routes require bearer auth. **Blobs are not owner-scoped**
— any authenticated user can read/write any blob by hash. Treat this as
a shared, content-addressed object pool.

> Binary content travels as base64 inside the JSON envelope. A native
> `application/octet-stream` surface may land later; for now frontends
> should base64-encode on upload and base64-decode on download.

### `POST /v1/blobs`

Upload bytes. Idempotent — if the blob already exists the server
returns `201` with the same `{hash, size}`.

**Request body**

```json
{ "content_base64": "aGVsbG8gd29ybGQ=" }
```

**Response 201 — BlobMeta**

```json
{
  "meta": { ... },
  "data": { "hash": "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9", "size": 11 }
}
```

**Errors**

| HTTP | Code | Meaning |
| --- | --- | --- |
| 400 | `bad_request` | `content_base64` is not valid base64 |

### `GET /v1/blobs/{hash}`

Fetch a blob's bytes.

**Response 200 — BlobPayload**

```json
{
  "meta": { ... },
  "data": {
    "hash":           "b94d27b9…",
    "size":           11,
    "content_base64": "aGVsbG8gd29ybGQ="
  }
}
```

**Errors**

| HTTP | Code | Meaning |
| --- | --- | --- |
| 404 | `not_found` | No blob stored under that hash |

### `GET /v1/blobs/{hash}/meta`

Check existence + size without downloading bytes.

**Response 200**

```json
{
  "meta": { ... },
  "data": { "hash": "b94d27b9…", "size": 11, "exists": true }
}
```

`exists` is always `true` on a 200. A missing blob returns `404
not_found`.

### `DELETE /v1/blobs/{hash}`

Remove the blob globally. Be careful: any repo that was referencing it
via a tree entry will now have a dangling pointer. (There is no GC
today; callers should know what they're doing.)

**Response 200**

```json
{ "meta": { ... }, "data": { "deleted": true, "hash": "b94d27b9…" } }
```

---

## 11. Endpoints — Trees

A tree is a directory snapshot: a list of entries, each pointing at
either a blob (file) or another tree (sub-directory). Trees are
identified by `sha256(canonical_form)` where the canonical form is
`<type> <hash> <name>\n` per entry, sorted by name. The canonicalisation
happens server-side — clients may submit entries in any order.

### `POST /v1/trees`

Create a tree. Idempotent on content.

**Request body**

```json
{
  "entries": [
    { "name": "README.md",  "entry_type": "blob", "hash": "b94d27b9…" },
    { "name": "src",        "entry_type": "tree", "hash": "9a2e1c7d…" }
  ]
}
```

| Field | Rules |
| --- | --- |
| `entries[].name` | non-empty, cannot contain `/` or be `.` / `..` |
| `entries[].entry_type` | `"blob"` or `"tree"` |
| `entries[].hash` | non-empty; refers to an existing blob/tree (server does not currently validate existence — dangling pointers are possible) |
| `entries` as a whole | names must be unique |

**Response 201 — TreeView**

```json
{
  "meta": { ... },
  "data": {
    "hash": "ca21…",
    "entries": [
      { "name": "README.md", "entry_type": "blob", "hash": "b94d27b9…" },
      { "name": "src",       "entry_type": "tree", "hash": "9a2e1c7d…" }
    ]
  }
}
```

Note that the server **returns entries sorted by name** (normalised
form), regardless of the order you submitted them in.

**Errors**

| HTTP | Code | Meaning |
| --- | --- | --- |
| 422 | `validation_failed` | Illegal entry name, type not `blob`/`tree`, empty hash, or duplicate name |

### `GET /v1/trees/{hash}`

Fetch a tree by its hash.

**Response 200 — TreeView**

### `GET /v1/trees/{hash}/flat`

Walk the whole tree recursively and return every leaf as
`(path, blob_hash)` pairs. Useful for building file browsers or
computing diffs client-side.

**Response 200**

```json
{
  "meta": { ... },
  "data": [
    { "path": "README.md",          "blob_hash": "b94d27b9…" },
    { "path": "src/main.rs",        "blob_hash": "4d3e08a5…" },
    { "path": "src/lib/utils.rs",   "blob_hash": "2e52d7da…" }
  ]
}
```

Paths use POSIX separators (`/`) regardless of the origin platform.

### `GET /v1/trees/{hash}/path?path=<posix/path>`

Resolve a single path inside a tree.

**Query**

- `path` *(required)* — POSIX path; leading/empty segments are ignored.

**Response 200**

```json
{
  "meta": { ... },
  "data": { "entry_type": "blob", "hash": "b94d27b9…" }
}
```

**Errors**

| HTTP | Code | Meaning |
| --- | --- | --- |
| 404 | `not_found` | Path doesn't exist inside the tree |

---

## 12. Endpoints — Commits

A commit is a `(repo_id, tree, parents[], message, timestamp)` tuple.
Commit IDs are deterministic: identical inputs yield the same hash
(idempotent). Reading by hash is open to any authenticated user;
*creating* a commit is scoped to a repo you own.

### `POST /v1/repos/{id}/commits`

Create a commit. Caller must own the repo.

**Request body**

```json
{
  "tree":    "ca21…",
  "parents": ["a14c1d…"],
  "message": "feat: add blob storage"
}
```

| Field | Rules |
| --- | --- |
| `tree` | non-empty tree hash |
| `parents` | array of commit hashes; `[]` for a root commit |
| `message` | non-empty after trimming whitespace |

**Response 201 — CommitView**

```json
{
  "meta": { ... },
  "data": {
    "hash":      "b22f48…",
    "repo_id":   "66211bf49a4d3a001deadb111",
    "tree":      "ca21…",
    "parents":   ["a14c1d…"],
    "message":   "feat: add blob storage",
    "timestamp": 1744934120
  }
}
```

This endpoint does **not** advance the repo's HEAD. If you want to make
the commit the new tip, follow up with `PUT /v1/repos/{id}/head`. The
high-level `index/commit` endpoint (§13) does both in one step.

### `GET /v1/repos/{id}/commits?limit=<n>`

List commits belonging to a repo, newest first.

**Query**

- `limit` *(optional, int)* — truncate after `n` commits. Default: no
  limit (the whole commits collection for the repo).

**Response 200** — `CommitView[]`

### `GET /v1/commits/{hash}`

Fetch a single commit. No repo context required.

**Response 200 — CommitView**

### `GET /v1/commits/{hash}/history?limit=<n>`

Walk the ancestry of a commit — BFS across `parents`, de-duplicated by
hash, sorted newest-first. This is what powers `ledger log`.

**Query**

- `limit` *(optional, int ≥ 1, default 100)* — maximum number of
  commits to return.

**Response 200** — `CommitView[]`

---

## 13. Endpoints — Staging index

Each repo has at most one staging index (`unique(repo_id)`). It is a
flat list of `(path, blob_hash)` entries waiting to become a commit.
Every route requires repo ownership.

### `GET /v1/repos/{id}/index`

Show the current staging index. If no index has been touched yet this
returns an empty, unsaved index.

**Response 200 — IndexView**

```json
{
  "meta": { ... },
  "data": {
    "id":      "66211df49a4d3a001deadb333",   // null if no index doc has been written yet
    "repo_id": "66211bf49a4d3a001deadb111",
    "entries": [
      { "path": "README.md",         "blob_hash": "b94d27b9…" },
      { "path": "src/main.rs",       "blob_hash": "4d3e08a5…" }
    ]
  }
}
```

### `POST /v1/repos/{id}/index`

Stage one entry (insert or replace by path).

**Request body**

```json
{ "path": "src/main.rs", "blob_hash": "4d3e08a5…" }
```

Rules:

- `path` is trimmed and stripped of leading/trailing `/`. Must be
  non-empty. Each segment must be non-empty, `.`, or `..` (those three
  are rejected).
- `blob_hash` must be non-empty. The server does not verify the blob
  exists — uploading the blob first is the client's responsibility.

**Response 202 — IndexView** (the full index after the stage)

### `DELETE /v1/repos/{id}/index?path=<posix/path>`

Remove one staged entry. No-op if the path isn't staged.

**Response 200 — IndexView** (the full index after the unstage)

### `DELETE /v1/repos/{id}/index/all`

Empty the staging index (but keep the index document).

**Response 200**

```json
{ "meta": { ... }, "data": { "cleared": true } }
```

### `POST /v1/repos/{id}/index/commit`

Turn the current staging index into a commit, then advance HEAD, then
clear the index. This is the one-shot "commit my changes" call.

**Request body**

```json
{ "message": "feat: first commit" }
```

**Response 201 — CommitView** (the new commit; see §12 for shape)

**Errors**

| HTTP | Code | Meaning |
| --- | --- | --- |
| 422 | `validation_failed` | Staging index is empty, or message is blank |
| 422 | `validation_failed` | A staged path conflicts with an already-staged directory (or vice-versa) |

---

## 14. Endpoints — Diff

Diffs compare two *flattened* trees: the set of `(path, blob_hash)`
pairs. A change is `added`, `modified`, or `deleted`.

### `GET /v1/diff/trees?left=<hash>&right=<hash>`

Diff two tree hashes. Either side may be `""` (empty string) to
represent an empty tree.

### `GET /v1/diff/commits?left=<hash>&right=<hash>`

Same, but the hashes are commits — the server dereferences each commit
to its `tree` and then delegates to the tree diff.

### Response 200 — `Change[]` (both endpoints)

```json
{
  "meta": { ... },
  "data": [
    {
      "path":     "src/main.rs",
      "kind":     "modified",
      "old_hash": "4d3e08a5…",
      "new_hash": "7b94a88b…"
    },
    {
      "path":     "src/new.rs",
      "kind":     "added",
      "old_hash": null,
      "new_hash": "2e52d7da…"
    },
    {
      "path":     "legacy/old.rs",
      "kind":     "deleted",
      "old_hash": "a14c1d…",
      "new_hash": null
    }
  ]
}
```

- Results are **sorted by path** for stable rendering.
- `kind` is one of `"added" | "modified" | "deleted"`.
- For `added`: `old_hash == null`, `new_hash != null`.
- For `deleted`: `old_hash != null`, `new_hash == null`.
- For `modified`: both hashes are present and differ.

---

## 15. Type glossary

One-line TypeScript definitions for every shape the API can return. All
shapes are wrapped in `Envelope<T>` unless otherwise noted.

```ts
// --- identity ---
type ObjectIdHex = string;   // 24 hex chars
type Sha256Hex   = string;   // 64 hex chars

// --- users / sessions ---
type UserView = {
  id: ObjectIdHex;
  username: string;
  created_at: number;              // unix seconds
  last_login_at: number | null;    // unix seconds or null if never
};

type TokenPair = {
  access_token: string;
  refresh_token: string;
  token_type: "Bearer";
  access_expires_in: number;       // seconds until access_token.exp
  refresh_expires_in: number;      // seconds until refresh_token.exp
  stay_logged_in: boolean;
};

type Session = { user: UserView; tokens: TokenPair };

type LoggedOut = { logged_out: true };

// --- repos ---
type RepoView = {
  id: ObjectIdHex;
  owner_id: ObjectIdHex;
  name: string;
  head_commit: Sha256Hex | null;
};

// --- refs ---
type RefView = {
  id: ObjectIdHex;
  repo_id: ObjectIdHex;
  name: string;
  commit: Sha256Hex;
};

// --- blobs ---
type BlobMeta    = { hash: Sha256Hex; size: number };
type BlobPayload = { hash: Sha256Hex; size: number; content_base64: string };

// --- trees ---
type TreeEntryType = "blob" | "tree";
type TreeEntry = { name: string; entry_type: TreeEntryType; hash: Sha256Hex };

type TreeView = { hash: Sha256Hex; entries: TreeEntry[] };

type FlatEntry    = { path: string; blob_hash: Sha256Hex };
type ResolveResult = { entry_type: TreeEntryType; hash: Sha256Hex };

// --- commits ---
type CommitView = {
  hash: Sha256Hex;
  repo_id: ObjectIdHex;
  tree: Sha256Hex;
  parents: Sha256Hex[];
  message: string;
  timestamp: number;   // unix seconds
};

// --- staging index ---
type IndexEntry = { path: string; blob_hash: Sha256Hex };
type IndexView  = {
  id: ObjectIdHex | null;
  repo_id: ObjectIdHex;
  entries: IndexEntry[];
};

// --- diff ---
type ChangeKind = "added" | "modified" | "deleted";
type Change = {
  path: string;
  kind: ChangeKind;
  old_hash: Sha256Hex | null;
  new_hash: Sha256Hex | null;
};

// --- generic delete/ack payloads ---
type Deleted  = { deleted: true; [k: string]: unknown };
type Cleared  = { cleared: true };
type HealthView = { status: "ok"; service: string; version: string };
```

---

## 16. HTTP status codes

| Status | When |
| --- | --- |
| `200 OK` | Any GET / PATCH / DELETE / PUT / most POSTs that mutate existing state |
| `201 Created` | Resource creation: register, repo, tree, commit, blob, ref, index/commit |
| `202 Accepted` | `POST /v1/repos/{id}/index` (single-entry stage) |
| `400 Bad Request` | Invalid JSON body, malformed path/query parameters |
| `401 Unauthorized` | Missing/invalid/expired token, or bad credentials on `/auth/login` |
| `403 Forbidden` | Reserved (not currently emitted — owner checks return 404 instead) |
| `404 Not Found` | Resource missing, or owned by someone else (same response) |
| `409 Conflict` | Unique violation (repo name, ref name) |
| `422 Unprocessable Entity` | Request validation failed |
| `500 Internal Server Error` | Unexpected server failure or database error |

Clients should branch on `errors[0].code` rather than HTTP status when
available — the code is stable across releases; status can change if the
team later decides to start emitting `403` for ownership.

---

## 17. Error code catalog

Every `code` the server will currently emit, the HTTP status it pairs
with, and when to expect it.

### Client input (400 / 422)

| Code | HTTP | When |
| --- | --- | --- |
| `bad_request` | 400 | Malformed path or query parameter (e.g. non-hex ObjectId) |
| `invalid_json` | 400 | Request body could not be parsed as JSON |
| `validation_failed` | 422 | Body parsed but violated a rule (length, charset, required, etc.). `source.pointer` / `source.parameter` is populated |

### Auth (401)

| Code | When |
| --- | --- |
| `missing_token` | `Authorization` header absent on a protected route |
| `invalid_scheme` | `Authorization` present but not `Bearer` |
| `invalid_token` | Token failed signature / shape validation |
| `token_expired` | Token past `exp` |
| `wrong_token_type` | Access token supplied where refresh expected (or vice versa) |
| `invalid_credentials` | `/auth/login` got wrong username/password |
| `token_reuse_detected` | Refresh token was already used; **every** active session for the user was just revoked |

### Resource state (404 / 409)

| Code | HTTP | When |
| --- | --- | --- |
| `not_found` | 404 | Resource missing — or the caller doesn't own it (for repo-scoped routes) |
| `conflict` | 409 | Unique invariant would be violated (repo name, ref name) |

### Server (500)

| Code | When |
| --- | --- |
| `internal_error` | Caught unexpected error / unencoded invariant broke |
| `database_error` | MongoDB driver/command returned an error |

---

## 18. Workflows

End-to-end request sequences a frontend typically needs.

### 18.1 Sign up, log in, use the API

```http
POST /v1/auth/register
Content-Type: application/json

{ "username": "alice", "password": "correct-horse-battery-staple" }
```

Persist `tokens.access_token` and `tokens.refresh_token` from the
response. Attach the access token as `Authorization: Bearer …` on every
subsequent call.

### 18.2 Refresh on 401

```
(any request)
  → 401  errors[0].code === "token_expired"

POST /v1/auth/refresh
Content-Type: application/json
{ "refresh_token": "<saved refresh>" }
  → 200  TokenPair

(retry original request with new access_token)
```

If refresh returns `401` with `token_reuse_detected`, prompt the user
to log in again and wipe local state.

### 18.3 Create a repo and make the first commit

```
POST /v1/repos                                 { name: "alice/notes" }             → RepoView { id }
POST /v1/blobs                                 { content_base64: "..." }           → BlobMeta { hash: H1 }
POST /v1/blobs                                 { content_base64: "..." }           → BlobMeta { hash: H2 }
POST /v1/repos/{id}/index                      { path: "README.md",  blob_hash: H1 } → IndexView
POST /v1/repos/{id}/index                      { path: "src/main.rs", blob_hash: H2 } → IndexView
POST /v1/repos/{id}/index/commit               { message: "initial" }              → CommitView { hash: C1 }
POST /v1/repos/{id}/refs                       { name: "main", commit: C1 }        → RefView
```

After the `index/commit` call, the repo's HEAD is automatically set to
`C1` and the staging index is cleared.

### 18.4 Show the file tree of HEAD

```
GET /v1/repos/{id}                    → RepoView { head_commit: C1 }
GET /v1/commits/C1                    → CommitView { tree: T1 }
GET /v1/trees/T1/flat                 → FlatEntry[]
```

To open a single file:

```
GET /v1/trees/T1/path?path=src/main.rs     → { entry_type: "blob", hash: B1 }
GET /v1/blobs/B1                            → BlobPayload { content_base64 }
```

### 18.5 Show a diff for a commit

```
GET /v1/commits/C2                             → CommitView { parents: [C1], tree: T2 }
GET /v1/diff/commits?left=C1&right=C2          → Change[]
```

For a root commit (no parents), diff against the empty tree:

```
GET /v1/diff/trees?left=&right=T1              → Change[]       (everything "added")
```

### 18.6 Create and switch branch

```
POST /v1/repos/{id}/refs        { name: "dev", commit: C1 }   → RefView
PUT  /v1/repos/{id}/head        { commit: C1 }                → RepoView
```

To "switch back" to `main`:

```
GET  /v1/repos/{id}/refs/main                                 → RefView { commit: Cx }
PUT  /v1/repos/{id}/head        { commit: Cx }                → RepoView
```

### 18.7 Log out cleanly

```
POST /v1/auth/logout   { refresh_token: "<saved refresh>" }   → { logged_out: true }
(clear local tokens)
```

---

## 19. Implementation notes

Details useful when building the frontend — mostly about edge cases.

### 19.1 Content addressing

| Object | `_id` formula |
| --- | --- |
| Blob | `sha256(content_bytes)` |
| Tree | `sha256(canonical_form)` where canonical = `"<type> <hash> <name>\n"` per entry, **sorted by name** |
| Commit | `sha256("repo <hex>\ntree <hash>\n(parent <hash>\n)* (sorted)timestamp <unix_secs>\n\n<message>")` |

Consequences:

- Repeated uploads of the same blob collapse into one document.
- Two trees with the same contents always have the same hash, regardless
  of entry order on the wire.
- Two commits with the same tree, parents, and message but different
  timestamps are **different** commits (the timestamp is part of the
  canonical form). Idempotency therefore only applies when the client
  retries *fast enough* that the server-side clock hasn't ticked — do
  not rely on commit idempotency across retries seconds apart.

### 19.2 Ownership model

- Every mutation of `repos/{id}/*` goes through a helper that first
  `GET`s the repo and checks `owner_id == auth.user.id`.
- On mismatch the server returns `404 not_found`, not `403`, to avoid
  leaking the existence of repos the caller doesn't own.
- Blobs, trees, and commits are **globally shared** once written (they
  have no `owner_id`). This keeps the data model content-addressable
  and dedup-friendly. If you want per-user privacy, keep blob hashes out
  of the URL surface you expose to other users.

### 19.3 Dangling references

The server does not currently verify that a tree's entries point at
blobs/trees that actually exist, or that a commit's `parents` exist.
Frontends should upload blobs **before** creating trees, and create
trees **before** creating commits. `POST /v1/repos/{id}/index/commit`
bundles all of this correctly; the manual `POST /v1/repos/{id}/commits`
path puts the invariant in the caller's hands.

### 19.4 Limits

- **Request body size.** The HTTP layer rejects any request body
  larger than `LEDGER_MAX_BODY_BYTES` (default `33_554_432` = 32 MiB).
  Requests over the limit get back a plain `413 Payload Too Large`
  with body text `"Failed to buffer the request body: length limit
  exceeded"`, **not** the standard JSON envelope — this is a
  tower-level layer that runs before the router. Size the frontend's
  uploads accordingly, or bump the env var on the server.
- **Blob size.** Bounded by MongoDB's 16 MiB document limit because
  `content` lives inline. In practice, keep raw blobs under ~12 MiB
  (base64 inflates 4/3×, JSON overhead adds a touch more).
- **Maximum entries per tree.** Same document limit.
- **Pagination.** No cursor-based pagination yet. `GET /v1/repos` and
  `GET /v1/repos/{id}/commits` return the full set; add `?limit=` on
  commit lists and history when you expect long tails.
- **Rate limiting.** None — put one at your proxy layer if you need it.

### 19.5 Timestamps

All server timestamps are **unix seconds** (64-bit integer). The `meta`
block uses RFC3339 millisecond-precision UTC for human readability.
Clients are responsible for their own locale conversion.

### 19.6 Environment / config knobs (server-side)

| Variable | Default | Effect |
| --- | --- | --- |
| `LEDGER_HTTP_ADDR` | `127.0.0.1:3030` | HTTP bind address |
| `LEDGER_MONGO_URI` | `mongodb://127.0.0.1:27017` | MongoDB connection |
| `LEDGER_DB_NAME` | `ledger` | MongoDB database |
| `LEDGER_JWT_SECRET` | *(random per boot, all tokens invalidated on restart)* | HS256 secret |
| `LEDGER_LOG` | `info,tower_http=info` | `tracing` filter |
| `LEDGER_ENV_FILE` | *(unset; discovered via `.env` walk)* | Explicit `.env` path |
| `LEDGER_CORS_ORIGIN` | `*` | CORS origin policy; `*` for any, or a comma-separated list of exact origins |
| `LEDGER_MAX_BODY_BYTES` | `33554432` (32 MiB) | Maximum HTTP request body size in bytes |

Frontends don't see these directly; they affect token lifetimes, base
URL, etc.

### 19.7 What the API does *not* provide (yet)

So you know what you still have to build client-side:

- User-to-user sharing / ACLs beyond "owner or 404".
- Real-time / websocket push.
- Partial blob uploads (chunking).
- Server-side diff rendering (the API returns a structured
  `Change[]`; human-readable unified diffs are a frontend concern).
- Search across messages / paths.
- Tags distinct from branches (`refs` are used for both today).

---

*Doc generated from `src/api/` and `src/core/` as of ledger
`0.1.0`. If you find something here that doesn't match the running
server, the running server wins — open an issue and fix this document.*
