# Unit Testing Plan — ferum-board Backend

## Status: Planned (not yet implemented)

This document captures the testability audit findings for the Rust backend and the concrete steps needed to enable unit testing. All use-case logic is correct and the repository pattern is already in place — two architectural gaps block testing today. Fix those gaps first, then write tests top-down.

---

## Audit Findings Summary

| Area | Status | Detail |
|------|--------|--------|
| `EventBus` as concrete struct | ❌ Blocker | Use cases hold `Arc<EventBus>`, cannot be swapped for a no-op |
| `State<AppState>` in handlers | ❌ Blocker | Handlers bundle all deps; impossible to test one handler in isolation |
| Repository dependencies | ✅ Clean | All repos are `Arc<dyn Repo>` — swap for mocks with no changes |
| `PermissionChecker` | ✅ Testable now | Pure static methods, no I/O |
| Plugin ports | ✅ Testable | `Arc<dyn PluginHookRuntime>` + `Arc<dyn PluginUiRuntime>` (ISP-split traits) |
| `AppError` / `HandlerError` | ✅ Assertion-friendly | `thiserror` enums, match ergonomically |
| Global / static state | ✅ None | No `lazy_static`, no `static mut` |
| Existing test infrastructure | ❌ None | Zero `#[cfg(test)]`, no test crates, no mock libraries |

---

## Step 1 — Introduce `EventPublisher` trait

### Problem
Every use case that fires events holds `Arc<EventBus>` (a concrete struct). `EventBus` has 7+ real dependencies (audit log, notifications, webhooks, plugin runtime, …). A test cannot construct it without a live database.

Affected use cases: `ThreadUseCase`, `PostUseCase`, `ReactionUseCase`, `ModerationUseCase`, and any future use case that calls `self.event_bus.publish(...)`.

### Solution
Extract a single-method trait in `ferum-application/src/ports.rs`:

```rust
#[async_trait]
pub trait EventPublisher: Send + Sync {
    async fn publish(&self, event: ForumEvent);
}
```

Implement it on `EventBus` (the real implementation):

```rust
// ferum-application/src/event_bus.rs
#[async_trait]
impl EventPublisher for EventBus {
    async fn publish(&self, event: ForumEvent) {
        self.handle(&event).await;   // existing logic, unchanged
    }
}
```

Change all use case struct fields from:

```rust
pub event_bus: Arc<EventBus>,
```

to:

```rust
pub event_bus: Arc<dyn EventPublisher>,
```

Provide a no-op implementation for tests (lives in `ferum-application/src/testing.rs` behind `#[cfg(test)]` or in a `ferum-test-support` crate):

```rust
pub struct NoopEventPublisher;

#[async_trait]
impl EventPublisher for NoopEventPublisher {
    async fn publish(&self, _event: ForumEvent) {}
}
```

Provide a spy variant when tests need to assert events were fired:

```rust
pub struct SpyEventPublisher {
    pub events: Arc<Mutex<Vec<ForumEvent>>>,
}

#[async_trait]
impl EventPublisher for SpyEventPublisher {
    async fn publish(&self, event: ForumEvent) {
        self.events.lock().unwrap().push(event);
    }
}
```

### Files to change
| File | Change |
|------|--------|
| `ferum-application/src/ports.rs` | Add `EventPublisher` trait |
| `ferum-application/src/event_bus.rs` | Implement `EventPublisher` for `EventBus` |
| `ferum-application/src/usecases/thread_usecase.rs` | `Arc<EventBus>` → `Arc<dyn EventPublisher>` |
| `ferum-application/src/usecases/post_usecase.rs` | same |
| `ferum-application/src/usecases/reaction_usecase.rs` | same |
| `ferum-application/src/usecases/moderation_usecase.rs` | same |
| `ferum-web/src/app_state.rs` | Pass `Arc<event_bus> as Arc<dyn EventPublisher>` via `Arc::clone` + coercion |
| `ferum-application/src/testing.rs` (new) | `NoopEventPublisher`, `SpyEventPublisher` |

### Effort estimate
~4 hours. Mechanical field-type changes + one trait impl. No logic changes.

---

## Step 2 — `AppState::for_test()` builder for handler tests

### Problem
Handlers are `async fn foo(State(state): State<AppState>, ...)`. `AppState` holds every use case, which holds every repository. You cannot test handler routing, input validation, or error mapping without a full database.

### Solution
Add a `for_test` constructor to `AppState` that accepts pre-built use cases (already constructed with mock repositories):

```rust
// ferum-web/src/app_state.rs
#[cfg(test)]
impl AppState {
    pub fn for_test() -> AppStateBuilder {
        AppStateBuilder::default()
    }
}

#[cfg(test)]
#[derive(Default)]
pub struct AppStateBuilder {
    pub thread: Option<Arc<ThreadUseCase>>,
    pub post:   Option<Arc<PostUseCase>>,
    // … one field per use case
}

#[cfg(test)]
impl AppStateBuilder {
    pub fn thread(mut self, uc: Arc<ThreadUseCase>) -> Self {
        self.thread = Some(uc); self
    }
    pub fn build(self) -> AppState {
        AppState {
            thread: self.thread.expect("thread use case required"),
            // … panic if caller forgot a dep
        }
    }
}
```

This lets a handler test do:

```rust
let state = AppState::for_test()
    .thread(Arc::new(thread_uc_with_mocks))
    .build();

let response = create_thread(State(state), Json(body)).await;
assert_eq!(response.status(), StatusCode::CREATED);
```

No Axum router needed; handlers are plain async functions and can be called directly.

### Files to change
| File | Change |
|------|--------|
| `ferum-web/src/app_state.rs` | Add `AppStateBuilder` under `#[cfg(test)]` |

### Effort estimate
~2 hours.

---

## Step 3 — Add `mockall` and mock repository implementations

### Problem
Use case tests need repository implementations that return controlled data without hitting a database.

### Solution
Add `mockall` as a dev-dependency and generate mocks for the key repository traits:

```toml
# ferum-application/Cargo.toml  [dev-dependencies]
mockall = "0.13"
tokio = { version = "1", features = ["rt", "macros"] }
```

Mark each repository trait with `#[cfg_attr(test, mockall::automock)]`:

```rust
// ferum-domain/src/repositories/thread_repository.rs
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait ThreadRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Thread>, AppError>;
    // …
}
```

This auto-generates `MockThreadRepository` which can be used in tests:

```rust
let mut mock_threads = MockThreadRepository::new();
mock_threads
    .expect_find_by_id()
    .returning(|_| Ok(Some(fixture_thread())));

let uc = ThreadUseCase::new(
    Arc::new(mock_threads),
    Arc::new(MockCategoryRepository::new()),
    Arc::new(MockPostRepository::new()),
    Arc::new(NoopJobQueue),
    Arc::new(MockStoredFileRepository::new()),
    Arc::new(NoopEventPublisher),   // from Step 1
    Arc::new(InMemoryCacheService::new()),
    Arc::new(MockTagRepository::new()),
    Arc::new(MockUserRepository::new()),
    Arc::new(NullPluginRuntime),   // implements PluginHookRuntime
);
```

### Files to change
| File | Change |
|------|--------|
| `ferum-domain/Cargo.toml` | Add `mockall` to `[dev-dependencies]` |
| All `*_repository.rs` trait files | Add `#[cfg_attr(test, mockall::automock)]` attribute |
| `ferum-application/Cargo.toml` | Add `mockall`, `tokio` test runtime to dev-deps |

### Effort estimate
~2 hours for wiring. `mockall` does the heavy lifting.

---

## Step 4 — Test fixtures and helper module

### Problem
Tests need realistic domain objects (`Thread`, `Post`, `User`, `AuthUser`) with correct default values. Building these by hand in every test is verbose and fragile.

### Solution
Create a `ferum-application/src/testing.rs` module (compiled only in `#[cfg(test)]`) that provides builders and fixtures:

```rust
#[cfg(test)]
pub mod fixtures {
    use uuid::Uuid;
    use ferum_domain::{AuthUser, models::{user::User, thread::Thread}};
    use std::collections::HashSet;

    pub fn admin_user() -> AuthUser {
        AuthUser {
            id: Uuid::new_v4(),
            username: "admin".into(),
            trust_level: TrustLevel::Regular,
            is_banned: false,
            banned_until: None,
            permissions: HashSet::from([
                "admin.users".into(),
                "thread.create".into(),
            ]),
            category_permissions: Default::default(),
        }
    }

    pub fn member_user() -> AuthUser {
        AuthUser {
            permissions: HashSet::from(["thread.create".into(), "post.create".into()]),
            ..admin_user()
        }
    }

    pub fn banned_user() -> AuthUser {
        AuthUser { is_banned: true, ..member_user() }
    }

    pub fn fixture_thread() -> Thread { /* … */ }
    pub fn fixture_post() -> Post { /* … */ }
}
```

### Files to create
- `ferum-application/src/testing.rs`

### Effort estimate
~2 hours. Grows incrementally as tests are added.

---

## Step 5 — First test suites (zero infrastructure needed)

These can be written immediately after Steps 1–4, with no database or container.

### 5a. `PermissionChecker` unit tests

`PermissionChecker` has pure static methods — no mocking needed at all.

```rust
// ferum-application/src/permission.rs (bottom of file)
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::fixtures::*;

    #[test]
    fn banned_user_cannot_create_thread() {
        let err = PermissionChecker::check_not_banned(&banned_user()).unwrap_err();
        assert!(matches!(err, AppError::Forbidden(_)));
    }

    #[test]
    fn admin_can_manage_plugins() {
        assert!(PermissionChecker::can_manage_plugins(&admin_user()).is_ok());
    }

    #[test]
    fn member_cannot_manage_plugins() {
        assert!(PermissionChecker::can_manage_plugins(&member_user()).is_err());
    }
}
```

### 5b. Use case logic tests (after Steps 1–3)

```rust
// ferum-application/src/usecases/thread_usecase.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{fixtures::*, NoopEventPublisher};
    use mockall::predicate::*;

    fn make_uc(threads: MockThreadRepository) -> ThreadUseCase {
        ThreadUseCase::new(
            Arc::new(threads),
            Arc::new(MockCategoryRepository::new()),
            Arc::new(MockPostRepository::new()),
            Arc::new(NoopJobQueue),
            Arc::new(MockStoredFileRepository::new()),
            Arc::new(NoopEventPublisher),
            Arc::new(InMemoryCacheService::new()),
            Arc::new(MockTagRepository::new()),
            Arc::new(MockUserRepository::new()),
            Arc::new(NullPluginRuntime),   // implements PluginHookRuntime
        )
    }

    #[tokio::test]
    async fn banned_user_cannot_create_thread() {
        let uc = make_uc(MockThreadRepository::new());
        let result = uc.create(&banned_user(), /* … */).await;
        assert!(matches!(result, Err(AppError::Forbidden(_))));
    }

    #[tokio::test]
    async fn create_thread_emits_thread_created_event() {
        let spy = Arc::new(SpyEventPublisher::default());
        // wire uc with spy instead of noop
        // …
        assert!(spy.events.lock().unwrap()
            .iter().any(|e| matches!(e, ForumEvent::ThreadCreated { .. })));
    }
}
```

### Effort estimate
~1 day for initial suite covering the happy path and main error cases of 3–4 use cases.

---

## Step 6 — Integration tests with `testcontainers` (optional, later)

For tests that must hit a real database (migration correctness, query correctness):

```toml
# backend/Cargo.toml [dev-dependencies]
testcontainers = "0.21"
testcontainers-modules = { version = "0.11", features = ["postgres"] }
```

```rust
#[tokio::test]
async fn create_thread_persists_to_db() {
    let docker = testcontainers::clients::Cli::default();
    let postgres = docker.run(testcontainers_modules::postgres::Postgres::default());
    let db_url = format!("postgres://postgres:postgres@localhost:{}/postgres",
        postgres.get_host_port_ipv4(5432));

    // run migrations
    // build real repositories
    // assert DB state after use case runs
}
```

These live in `backend/tests/` (Rust integration test directory), separate from unit tests.

### Effort estimate
~1 day setup + ongoing per feature.

---

## Implementation Order

```
Step 1 — EventPublisher trait          (~4h)  ← unblocks all use case tests
Step 2 — AppState::for_test()          (~2h)  ← unblocks handler tests
Step 3 — mockall + automock attrs      (~2h)  ← makes mock construction easy
Step 4 — fixtures module               (~2h)  ← eliminates test boilerplate
Step 5 — first test suites             (~1d)  ← validates everything works
Step 6 — testcontainers (optional)     (~1d)  ← only when repo-layer coverage needed
                                       ─────
Total to unit-testable codebase:       ~2.5 days
```

Steps 1–4 are pure infrastructure with no behavior changes. The codebase compiles and runs identically after them. Only Step 5 introduces actual test coverage.

---

## What NOT to test

- Sea-ORM entity mappings — the ORM guarantees type safety at compile time
- `entity_to_domain` mapping functions — these have no business logic; a type error would fail the compiler
- Handler input parsing — Axum's `Json<T>` extractor and `validator` crate handle this; trust the framework
- Permission resolution in middleware — this is already tested by `PermissionChecker` unit tests; do not duplicate
