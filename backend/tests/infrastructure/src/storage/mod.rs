#[cfg(test)] mod database;
// The invariant tying `ports::file_url` (what we persist) to every adapter's
// `key_from_url` (what resolves it). Needs no database.
#[cfg(test)] mod file_url_contract;
// Not feature-gated: the probe answers for whichever backend is live.
#[cfg(test)] mod public_read;
// Mirror each adapter's own gating: every object store is opt-in, so their
// tests are too. Run with
// `cargo test -p ferum-infrastructure-tests --features gcs,s3,r2`.
#[cfg(all(test, feature = "gcs"))] mod gcs;
#[cfg(all(test, feature = "r2"))] mod r2;
#[cfg(all(test, feature = "s3"))] mod s3;
