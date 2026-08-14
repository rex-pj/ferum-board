//! The contract that makes storing `file_url` instead of `public_url` safe.
//!
//! `ports::file_url` is what this application *persists* — into `site_config`,
//! `themes.preview_url`, and the stored HTML of every post. `public_url` is only
//! where the bytes happen to live today. That split is what lets a deployment
//! change bucket, CDN or backend without rewriting its own archive.
//!
//! It rests on two invariants, checked here for every adapter under every
//! configuration that adapter supports:
//!
//! ```text
//! key_from_url(file_url(key))    == Some(key)     // the persisted form
//! key_from_url(public_url(key))  == Some(key)     // the served form
//! ```
//!
//! **Both directions, and the second is not redundant.** The first was once the
//! only one checked, and it passed while GCS configured with `GCS_PREFIX` behind
//! a CDN mounted at a path ending in `/files` returned a key still carrying the
//! prefix — because the resolver-path rule matched inside the *base*. Nothing
//! errored: `public_url` is handed to clients by `handlers/api/uploads.rs`, so a
//! key that names nothing simply left the reference uncounted.
//!
//! The third invariant has no round trip, because it is about what must **not**
//! be recognised:
//!
//! ```text
//! key_from_url(<a URL on somebody else's host>) == None
//! ```
//!
//! `key_from_url` feeds `decrement_ref` and `delete_by_key` (thread thumbnail
//! replacement, avatar and cover replacement), so claiming a foreign URL
//! releases a CAS reference that has nothing to do with it.

#![allow(unused_imports)]

use ferum_application::ports::{file_url, StorageService, FILES_PREFIX};

/// One key per CAS namespace shape in use: a plain prefix, the hyphenated
/// attachment namespace, and a plugin's dynamic `plugin_{slug}` prefix.
const KEYS: &[&str] = &[
    "avatars/0123456789abcdef0123456789abcdef.png",
    "post-attachments/fedcba9876543210fedcba9876543210.jpg",
    "plugin_community-polls/aaaabbbbccccddddeeeeffff00001111.webp",
    "theme-previews/11112222333344445555666677778888.webp",
];

/// Asserts both round trips for every key. `label` names the configuration, so a
/// failure says which of the matrix rows broke rather than just which adapter.
fn assert_round_trips(svc: &dyn StorageService, label: &str) {
    for key in KEYS {
        let persisted = file_url(key);
        assert_eq!(
            svc.key_from_url(&persisted).as_deref(),
            Some(*key),
            "[{label}] lost the PERSISTED form of `{key}` (`{persisted}`)"
        );

        let served = svc.public_url(key);
        assert_eq!(
            svc.key_from_url(&served).as_deref(),
            Some(*key),
            "[{label}] lost the SERVED form of `{key}` (`{served}`)"
        );
    }
}

/// Asserts the adapter claims nothing that is not ours.
///
/// `extra` carries the shapes that are only meaningful for one backend (another
/// tenant's bucket, say).
fn assert_rejects_foreign(svc: &dyn StorageService, label: &str, extra: &[String]) {
    let key = KEYS[0];
    let mut foreign = vec![
        // The case the old unanchored `/files/` scan accepted.
        format!("https://evil.example.com/files/{key}"),
        format!("http://evil.example.com/files/{key}"),
        // A host that merely *starts with* our CDN's name. Classic prefix-match
        // bug: matching the base without requiring a `/` boundary after it lets
        // an attacker register `cdn.example.com.evil.com`.
        format!("https://cdn.example.com.evil.com/files/{key}"),
        format!("https://cdn.example.com.evil.com/{key}"),
    ];
    foreign.extend_from_slice(extra);

    for url in &foreign {
        assert_eq!(
            svc.key_from_url(url),
            None,
            "[{label}] claimed a URL that is not ours: `{url}`"
        );
    }
}

#[test]
fn file_url_is_the_files_prefix_plus_the_key() {
    // Pinning the shape itself: this string is written into post HTML that is
    // never rewritten, so changing it silently orphans everything already
    // stored. If this assertion ever needs updating, that is the signal to stop
    // and think about the rows already holding the old form.
    assert_eq!(file_url("avatars/a.png"), "/files/avatars/a.png");
    assert_eq!(FILES_PREFIX, "/files/");
}

/// The default backend, and since it shares `url_shapes` with the object stores
/// it is covered by the same matrix.
///
/// `DatabaseConnection::default()` is sea-orm's disconnected variant — enough to
/// construct the adapter, and `key_from_url`/`public_url` are pure string
/// functions that never reach it.
mod database {
    use super::*;
    use ferum_infrastructure::storage::DatabaseStorageService;
    use sea_orm::DatabaseConnection;

    fn service(cdn: Option<&str>) -> DatabaseStorageService {
        DatabaseStorageService::new(DatabaseConnection::default()).with_cdn_base_url(cdn)
    }

    #[test]
    fn round_trips_under_every_cdn_setting() {
        assert_round_trips(&service(None), "database, no cdn");
        assert_round_trips(
            &service(Some("https://cdn.example.com")),
            "database, plain cdn",
        );
        // A CDN mounted at a path. `public_url` becomes
        // `{cdn}/files/{key}`, so the base itself now contains `/files/`.
        assert_round_trips(
            &service(Some("https://cdn.example.com/assets")),
            "database, cdn on a subpath",
        );
    }

    #[test]
    fn rejects_urls_on_other_hosts() {
        assert_rejects_foreign(&service(None), "database, no cdn", &[]);
        assert_rejects_foreign(
            &service(Some("https://cdn.example.com")),
            "database, plain cdn",
            &[],
        );
    }
}

#[cfg(feature = "s3")]
mod s3 {
    use super::*;
    use ferum_infrastructure::storage::S3StorageService;

    const ENDPOINT: &str = "http://minio:9000";

    async fn service(cdn: Option<&str>) -> S3StorageService {
        S3StorageService::new(ENDPOINT, "access", "secret", "forum-uploads", "us-east-1", cdn).await
    }

    #[tokio::test]
    async fn round_trips_under_every_cdn_setting() {
        // No CDN: `public_url` is the bucket-qualified endpoint root, and
        // `/files/{key}` is a shape this adapter never produces but must read.
        assert_round_trips(&service(None).await, "s3, no cdn");
        assert_round_trips(
            &service(Some("https://cdn.example.com")).await,
            "s3, plain cdn",
        );
        // A CDN whose path ends in `/files`, which is the natural choice for an
        // operator moving off database storage who wants URL shapes preserved.
        assert_round_trips(
            &service(Some("https://cdn.example.com/files")).await,
            "s3, cdn mounted at /files",
        );
        // A CDN pointed straight at the endpoint makes it a strict prefix of the
        // bucket root — the longest-first ordering in `accepted_bases` is what
        // keeps `forum-uploads/` out of the recovered key.
        assert_round_trips(&service(Some(ENDPOINT)).await, "s3, cdn == endpoint");
    }

    #[tokio::test]
    async fn rejects_urls_on_other_hosts() {
        for cdn in [None, Some("https://cdn.example.com")] {
            assert_rejects_foreign(
                &service(cdn).await,
                "s3",
                &[
                    // Right vendor, wrong bucket.
                    format!("{ENDPOINT}/someone-elses-bucket/{}", KEYS[0]),
                ],
            );
        }
    }
}

#[cfg(feature = "gcs")]
mod gcs {
    use super::*;
    use ferum_infrastructure::storage::GcsStorageService;

    const CREDENTIALS: &str = r#"{
        "type": "authorized_user",
        "client_id": "test.apps.googleusercontent.com",
        "client_secret": "not-a-real-secret",
        "refresh_token": "1//not-a-real-token"
    }"#;

    fn service(prefix: Option<&str>, cdn: Option<&str>) -> GcsStorageService {
        GcsStorageService::new("forum-uploads", prefix, cdn, Some(CREDENTIALS), None)
            .expect("valid credentials")
    }

    /// The full prefix × CDN matrix. GCS is the only adapter where the two axes
    /// interact — `GCS_PREFIX` belongs to the object's *location* and must be
    /// stripped coming back, while a `/files/` path under the same CDN never had
    /// one applied. Telling those apart is the whole job.
    #[test]
    fn round_trips_across_the_prefix_and_cdn_matrix() {
        for prefix in [None, Some("ferum"), Some("a/b")] {
            for cdn in [
                None,
                Some("https://cdn.example.com"),
                // The row that was broken: the base ends in `/files`, so a rule
                // that scanned for `/files/` anywhere matched the base and
                // returned the key with `GCS_PREFIX` still attached.
                Some("https://cdn.example.com/files"),
            ] {
                let label = format!(
                    "gcs, prefix={}, cdn={}",
                    prefix.unwrap_or("<none>"),
                    cdn.unwrap_or("<none>")
                );
                assert_round_trips(&service(prefix, cdn), &label);
            }
        }
    }

    /// Every Google-owned shape an object of ours can appear under, all of which
    /// `public_url` never mints but `key_from_url` must still read: the console's
    /// download links, virtual-hosted addressing, the mTLS endpoints, and any of
    /// them signed.
    #[test]
    fn round_trips_the_shapes_google_itself_hands_out() {
        let svc = service(Some("ferum"), None);
        let key = KEYS[0];
        let object = format!("ferum/{key}");
        for url in [
            format!("https://storage.googleapis.com/forum-uploads/{object}"),
            format!("https://forum-uploads.storage.googleapis.com/{object}"),
            format!("https://storage.cloud.google.com/forum-uploads/{object}"),
            format!("https://storage.mtls.googleapis.com/forum-uploads/{object}"),
            format!("https://c.storage.googleapis.com/forum-uploads/{object}"),
            // V4 signed: authorization lives in the query string, which is no
            // part of the key.
            format!(
                "https://storage.googleapis.com/forum-uploads/{object}\
                 ?X-Goog-Algorithm=GOOG4-RSA-SHA256&X-Goog-Signature=deadbeef"
            ),
        ] {
            assert_eq!(
                svc.key_from_url(&url).as_deref(),
                Some(key),
                "GCS failed to read a shape Google itself serves: `{url}`"
            );
        }
    }

    #[test]
    fn rejects_urls_on_other_hosts_and_other_buckets() {
        for prefix in [None, Some("ferum")] {
            for cdn in [None, Some("https://cdn.example.com")] {
                assert_rejects_foreign(
                    &service(prefix, cdn),
                    "gcs",
                    &[
                        // Right vendor, someone else's bucket — path-style and
                        // virtual-hosted.
                        format!("https://storage.googleapis.com/other-bucket/{}", KEYS[0]),
                        format!("https://other-bucket.storage.googleapis.com/{}", KEYS[0]),
                    ],
                );
            }
        }
    }

    /// A `CDN_BASE_URL` naming a Google host must not shadow the vendor rules.
    ///
    /// Every path-style entry in `native_prefixes` starts with
    /// `https://storage.googleapis.com`, so a CDN base of exactly that string is
    /// a strict prefix of all of them. When the CDN was consulted first, the
    /// genuine path-style URL resolved to an object name that began with the
    /// *bucket*, failed the `GCS_PREFIX` strip, and returned `None` — the file
    /// went unrecognised and its reference uncounted, with nothing logged.
    #[test]
    fn a_cdn_naming_a_google_host_does_not_shadow_the_bucket_rules() {
        let svc = service(Some("ferum"), Some("https://storage.googleapis.com"));
        let key = KEYS[0];
        assert_eq!(
            svc.key_from_url(&format!(
                "https://storage.googleapis.com/forum-uploads/ferum/{key}"
            ))
            .as_deref(),
            Some(key),
            "a CDN base equal to the vendor host must not hide the path-style rule"
        );
        // And the full contract still holds under that configuration.
        assert_round_trips(&svc, "gcs, cdn == vendor host");
    }

    /// The bucket-less vendor host is refused as a CDN base rather than honoured.
    ///
    /// `public_url` is `{base}/{object name}`, so honouring it would mint
    /// `https://storage.googleapis.com/ferum/{key}` — which is not a 404. The
    /// first path segment of a path-style GCS URL *is* the bucket, so that URL
    /// addresses a bucket named `ferum` owned by whoever registered it. Falling
    /// back to the correct path-style origin is the only safe reading.
    #[test]
    fn a_bucketless_vendor_host_is_rejected_as_a_cdn_base() {
        let key = KEYS[0];
        let svc = service(Some("ferum"), Some("https://storage.googleapis.com"));
        assert_eq!(
            svc.public_url(key),
            format!("https://storage.googleapis.com/forum-uploads/ferum/{key}"),
            "the bucket segment must survive a bogus CDN base"
        );

        // The shapes that *do* name the bucket are legitimate and must be kept.
        for base in [
            "https://storage.googleapis.com/forum-uploads",
            "https://forum-uploads.storage.googleapis.com",
        ] {
            let svc = service(Some("ferum"), Some(base));
            assert_eq!(
                svc.public_url(key),
                format!("{base}/ferum/{key}"),
                "`{base}` already names the bucket and must be honoured"
            );
            assert_round_trips(&svc, base);
        }
    }

    #[test]
    fn an_object_outside_our_prefix_is_not_ours() {
        // A bucket shared with another workload. Claiming its objects would
        // decrement a reference count belonging to something else.
        let svc = service(Some("ferum"), None);
        assert_eq!(
            svc.key_from_url(&format!(
                "https://storage.googleapis.com/forum-uploads/other-app/{}",
                KEYS[0]
            )),
            None
        );
    }
}

#[cfg(feature = "r2")]
mod r2 {
    use super::*;
    use ferum_infrastructure::storage::R2StorageService;

    const ACCOUNT: &str = "abc123def456";
    const BUCKET: &str = "forum-uploads";
    const API_ROOT: &str = "https://abc123def456.r2.cloudflarestorage.com";

    async fn service(public_base: &str, cdn: Option<&str>) -> R2StorageService {
        R2StorageService::new(
            ACCOUNT,
            None,
            BUCKET,
            "access",
            "secret",
            Some(public_base),
            cdn,
        )
        .await
        .expect("valid R2 configuration")
    }

    /// Unlike the other three backends there is no "no public origin" row: R2
    /// cannot be constructed without one, which is the whole point of the
    /// adapter. The axes that remain are which kind of public origin, and
    /// whether a legacy `CDN_BASE_URL` is also being read.
    #[tokio::test]
    async fn round_trips_across_the_public_base_and_legacy_cdn_matrix() {
        for public_base in [
            "https://cdn.example.com",
            // The natural choice when moving off database storage with URL
            // shapes preserved — the base itself ends in `/files`.
            "https://cdn.example.com/files",
            // The Cloudflare-managed development URL.
            "https://pub-0123456789abcdef.r2.dev",
        ] {
            for cdn in [None, Some("https://old-cdn.example.com")] {
                assert_round_trips(
                    &service(public_base, cdn).await,
                    &format!("r2, public={public_base}, cdn={}", cdn.unwrap_or("<none>")),
                );
            }
        }
    }

    /// R2 is multi-tenant on one hostname pattern, so the account id and the
    /// bucket are the only things separating our objects from a stranger's.
    #[tokio::test]
    async fn rejects_urls_on_other_accounts_and_other_buckets() {
        for cdn in [None, Some("https://old-cdn.example.com")] {
            assert_rejects_foreign(
                &service("https://cdn.example.com", cdn).await,
                "r2",
                &[
                    // Right vendor and bucket, wrong account.
                    format!(
                        "https://someone-else.r2.cloudflarestorage.com/{BUCKET}/{}",
                        KEYS[0]
                    ),
                    format!(
                        "https://{BUCKET}.someone-else.r2.cloudflarestorage.com/{}",
                        KEYS[0]
                    ),
                    // Right vendor and account, wrong bucket.
                    format!("{API_ROOT}/someone-elses-bucket/{}", KEYS[0]),
                    format!(
                        "https://someone-elses-bucket.abc123def456.r2.cloudflarestorage.com/{}",
                        KEYS[0]
                    ),
                    // An account id that merely starts with ours.
                    format!(
                        "https://abc123def456evil.r2.cloudflarestorage.com/{BUCKET}/{}",
                        KEYS[0]
                    ),
                ],
            );
        }
    }

    /// The one thing no other backend has to promise: the served URL is never
    /// under the signed-only API endpoint, whatever else is configured.
    #[tokio::test]
    async fn the_served_url_is_never_the_signed_only_api_endpoint() {
        for cdn in [None, Some(API_ROOT)] {
            let svc = service("https://cdn.example.com", cdn).await;
            for key in KEYS {
                let served = svc.public_url(key);
                assert!(
                    !served.contains("r2.cloudflarestorage.com"),
                    "minted a signed-only URL for `{key}`: {served}"
                );
            }
        }
    }
}

