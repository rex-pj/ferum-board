pub mod bcrypt_password_hasher;
pub mod network_utils;
pub mod plugins;
pub mod role_permission_cache;
#[cfg(feature = "bulk_seed")]
pub mod bulk_seed_service;
pub mod cache;
pub mod crypto;
pub mod email;
// `PluginStatus` has an `Error` variant, and `DeriveActiveEnum` generates a
// `TryFrom` whose `Self::Error` is then ambiguous with `TryFrom::Error`. The
// compiler reads it correctly, but the lint is deny-by-default.
//
// The allow lives HERE, not in the generated file: `regen-entities.ps1`
// overwrites everything under `entities/`, so it would vanish on the next run.
#[allow(ambiguous_associated_items)]
pub mod entities;
pub mod i18n;
pub mod job_queue;
pub mod jwt_token_service;
pub mod notification;
pub mod observability;
pub mod rate_limit;
pub mod repositories;
pub mod search;
pub mod storage;
pub mod system_seed_service;
pub mod webhook_delivery;
