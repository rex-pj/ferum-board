pub mod bcrypt_password_hasher;
pub mod network_utils;
pub mod plugins;
pub mod role_permission_cache;
#[cfg(feature = "bulk_seed")]
pub mod bulk_seed_service;
pub mod cache;
pub mod crypto;
pub mod email;
// `sea_orm_active_enums::PluginStatus` has a variant named `Error` (the
// `plugin_status` DB value 'error'). sea-orm 2.0's `DeriveActiveEnum` expands to
// a sibling `impl TryFrom<&str>` whose signature says `Self::Error`, which is
// then ambiguous between that variant and `TryFrom::Error`. The compiler resolves
// it to the associated type — the correct reading — but
// `ambiguous_associated_items` is deny-by-default.
//
// The allow lives HERE, on the module declaration, rather than inside the
// generated file: everything under `entities/` is overwritten wholesale by
// `scripts/regen-entities.ps1`, so an attribute added there would vanish on the
// next regeneration and the build would break again with no obvious cause.
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
