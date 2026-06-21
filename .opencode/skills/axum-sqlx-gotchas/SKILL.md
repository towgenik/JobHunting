---
name: axum-sqlx-gotchas
description: Two gotchas hit during M3 — uuid serde feature missing and axum Handler not satisfied
---

# axum + sqlx + uuid — Two M3 Gotchas

## Gotcha 1: `uuid` crate needs `serde` feature for `Path<Uuid>` extractors

**Problem:** axum `Path<Uuid>` extractors fail with:
```
error[E0277]: the trait bound `Uuid: serde::Deserialize<'de>` is not satisfied
= note: required for `axum::extract::Path<Uuid>` to implement `FromRequestParts<AppState>`
```
This surfaces as `Handler<_, _>` not satisfied on the route — very confusing indirect error.

**Root cause:** `uuid` crate's `Deserialize` impl is behind the `serde` feature flag. The Architecture spec only listed `features = ["v4"]`.

**Fix:** In `Cargo.toml`:
```toml
uuid = { version = "1", features = ["v4", "serde"] }
```

**ponytail:** The error message says "Handler not satisfied" — not "Deserialize not implemented." Always add `#[axum::debug_handler]` to a failing handler to get the real cause. The debug_handler macro expands the trait bound chain and surfaces the actual missing impl.

---

## Gotcha 2: axum 0.7 extractor ordering — `State` must come before `Path`

**Problem:** Handlers with `Path<Uuid>` as first arg and `State<AppState>` second fail with "Handler not satisfied". Even after fixing the uuid serde feature, the ordering matters.

**Root cause:** In axum 0.7, `State` must come before `Path` (and other extractors) in the handler signature. `State` is not a "consuming" extractor but it must be first.

**Fix:**
```rust
// Wrong
async fn my_handler(Path(id): Path<Uuid>, State(app): State<AppState>) -> Response { ... }

// Correct
async fn my_handler(State(app): State<AppState>, Path(id): Path<Uuid>) -> Response { ... }
```

**ponytail:** axum 0.7 extractor ordering rule: `State` → path/query extractors → body extractors (`Form`, `Json`). Body extractors must always come last.

---

## Gotcha 3: `sqlx-cli` is not installed by default

**Problem:** `make migrate` runs `sqlx database create && sqlx migrate run` but `sqlx` binary is not on PATH.

**Fix:**
```bash
cargo install sqlx-cli --no-default-features --features sqlite
```
The `--no-default-features --features sqlite` flag keeps the install fast (skips postgres/mysql).

**ponytail:** sqlx-cli 0.9.x installs two binaries: `sqlx` and `cargo-sqlx`. Either works. The flag `--no-default-features --features sqlite` is 3× faster than the default build.
