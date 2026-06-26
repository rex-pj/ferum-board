pub mod database;
#[cfg(feature = "s3")]
pub mod s3;

pub use database::DatabaseStorageService;
#[cfg(feature = "s3")]
pub use s3::S3StorageService;
