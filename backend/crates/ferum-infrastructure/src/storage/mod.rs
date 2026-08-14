pub mod database;
#[cfg(feature = "gcs")]
pub mod gcs;
// Not feature-gated: it answers for whichever backend is live, and a CDN in
// front of database storage is just as capable of being misconfigured.
pub mod public_read;
#[cfg(feature = "r2")]
pub mod r2;
#[cfg(feature = "s3")]
pub mod s3;
// The `aws-sdk-s3` client `s3` and `r2` share. Private: it is an implementation
// detail of those two adapters, not a fourth backend. Compiled when either is,
// which is why the gate is `any` rather than a copy of one of them.
#[cfg(any(feature = "s3", feature = "r2"))]
mod s3_client;

// The resolver-path and CDN-base rules are identical in all four backends and
// are the ones whose failure mode is silent, so they are written once here
// rather than per adapter. Not feature-gated: `DatabaseStorageService` used to
// keep its own simpler copy, which is exactly how one of them came to accept a
// `/files/` path on any host in the world while the others were being tightened.
mod url_shapes;

pub use database::DatabaseStorageService;
pub use public_read::{probe_public_read, PublicReadProbe};
#[cfg(feature = "gcs")]
pub use gcs::GcsStorageService;
#[cfg(feature = "r2")]
pub use r2::R2StorageService;
#[cfg(feature = "s3")]
pub use s3::S3StorageService;
