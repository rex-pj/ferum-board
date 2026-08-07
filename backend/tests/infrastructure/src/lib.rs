pub mod common;

// ─── Top-level infrastructure components ──────────────────────────────────────
#[cfg(test)] mod bcrypt_password_hasher;
#[cfg(test)] mod jwt_token_service;
#[cfg(test)] mod network_utils;
#[cfg(test)] mod bulk_seed_service;
#[cfg(test)] mod system_seed_service;

// ─── Submodules mirroring source layout ───────────────────────────────────────
mod cache;
mod i18n;
mod job_queue;
mod notification;
mod plugins;
mod storage;
mod rate_limit;
mod repositories;
mod search;
