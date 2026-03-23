# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build --verbose          # Build
cargo test --verbose           # Run all tests
cargo test <test_name>         # Run a single test (e.g., cargo test limiter_test)
cargo test -p loc-place-bot    # Run tests for this crate specifically
```

Tests use `testcontainers` to spin up real Redis instances — no mocking of Redis.

Proto compilation runs automatically via `build.rs` on `cargo build`. It uses `tonic_prost_build::configure()` (not `tonic_build`) and compiles from `user-service-proto/`.

## Architecture

### Bot dispatch chain (`src/main.rs` → `src/handlers/mod.rs`)

`dptree` routes updates through 7 branches in order:
1. Inline queries → rate limiter → `SearchChain` → inline results
2. Chosen inline results → metrics
3. Location dialogue state machine (`LocationState::Start` / `Requested`)
4. Regular commands (`/start`, `/help`, `/loc`, `/setlanguage`, `/setlocation`)
5. Plain messages → `SearchChain` → coordinate buttons
6. Consent callbacks (EULA acceptance → user registration)
7. Cancellation / coordinate callbacks

### Location finding (`src/loc/`)

`SearchChain` calls a list of `LocFinder` implementations in order, merging results:
- `GoogleLocFinder` — Text Search or Geocoding API (controlled by `GAPI_MODE`)
- `OpenStreetMapLocFinder` — Nominatim reverse geocoding
- `YandexLocFinder` — Geocoder or Places API (controlled by `YAPI_MODE`)

Any finder can be disabled via `DISABLE_FINDER_{GOOGLE,OSM,YANDEX}=true`. For Russian (`ru`/`uk`/`be`) the chain order is reversed to prefer Yandex. Each finder is wrapped with `http-cache-reqwest` backed by `RedisCacheManager` (`src/loc/cache.rs`).

### HTTP cache (`src/loc/cache.rs`)

`RedisCacheManager` implements `http_cache::CacheManager`. Cache key: `loc-cache:{method}:{uri}:{body_sha256}`. Values are postcard-serialized `(HttpResponse, CachePolicy)` stored in Redis with TTL (`CACHE_MAX_TTL_SECS`, default 86400 s). An axum middleware (`InsertBodyHashIntoHeadersMiddleware`) computes the body hash before the cache layer sees the request.

### Rate limiter (`src/handlers/limiter.rs`)

Redis key `rate-limiter.{user_id}`: atomic INCR + EXPIRE pipeline. Configurable via `REQUESTS_LIMITER_MAX_ALLOWED` and `REQUESTS_LIMITER_TIMEFRAME`. Fails open on Redis errors.

### User service (`src/users/`)

gRPC client wrapping a tonic channel to `GRPC_ADDR_USER_SERVICE`. An in-memory CHashMap caches user records with TTL (`USER_CACHE_TIME_SECS`). The client is wrapped in `UserService::Connected(T)` / `UserService::Disabled` so the bot works without the user service. A periodic task spawned in `main.rs` calls `clean_up_cache()`.

### Deployment modes

- **Webhook** (production): `WEBHOOK_URL` set → axum router on port 8080 combines bot webhook route + `/metrics`
- **Polling** (development): `WEBHOOK_URL` unset → standard teloxide polling + separate metrics server

## Key dependency constraints

- **bincode**: pinned to `2.0.1` — do NOT upgrade (3.0.0 is a tombstone release)
- **tonic 0.14**: uses the `tonic-prost-build` / `tonic-prost` split; `build.rs` must call `tonic_prost_build::configure()`
- **reqwest 0.13**: TLS feature is `rustls` (not `rustls-tls`)
- **http-cache 1.0.0-alpha.5**: requires feature `url-standard` when `default-features = false`
- **redis 0.28** (via mobc-redis 0.9): `Value::Bulk` → `Value::Array`; `expire()` takes `i64`; `Value::Int(val)` — `val` is `i64` by value

## Environment variables

See `.env.example` for full list. Key ones:
- `TELOXIDE_TOKEN`, `GOOGLE_MAPS_API_KEY`, `YANDEX_MAPS_GEOCODER_API_KEY`
- `WEBHOOK_URL` — empty = polling mode
- `GAPI_MODE` (`GeoText` or `Geocode`), `YAPI_MODE` (`Place` or `Geocode`)
- `QUERY_CHECK_MODE` (`regex` or `empty`) — inline query validation strategy
- `GRPC_ADDR_USER_SERVICE` — set to empty to disable user service
