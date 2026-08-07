//! Tests for the anonymous-read startup probe.
//!
//! The whole value of the probe is one distinction: an object store answers an
//! anonymous request for a missing object with `404` when reads are open and
//! `403` when they are not. Getting that backwards would be worse than having no
//! probe at all — it would print "uploads are publicly readable" over a
//! deployment whose every image is about to 403.
//!
//! So these run against a real socket rather than asserting on a mocked status
//! code. The server is thirty lines of raw HTTP: no new dependency, and nothing
//! between the assertion and the behaviour.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use ferum_application::ports::StorageService;
use ferum_application::shared::AppError;
use ferum_infrastructure::storage::{probe_public_read, PublicReadProbe};

/// Minimal `StorageService` whose only real method is `public_url` — the only
/// one the probe calls.
struct StubStorage {
    base: String,
}

#[async_trait]
impl StorageService for StubStorage {
    async fn put(&self, _key: &str, _data: Bytes, _content_type: &str) -> Result<(), AppError> {
        unreachable!("the probe never writes")
    }
    async fn delete(&self, _key: &str) -> Result<(), AppError> {
        unreachable!("the probe never deletes")
    }
    fn public_url(&self, key: &str) -> String {
        format!("{}/{}", self.base, key)
    }
    fn key_from_url(&self, _url: &str) -> Option<String> {
        unreachable!("the probe never parses URLs")
    }
}

/// Answers exactly one request with `status_line`, then closes.
async fn stub_origin(status_line: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            // Drain the request line and headers; the probe sends no body.
            let mut buf = [0u8; 2048];
            let _ = socket.read(&mut buf).await;
            let response =
                format!("HTTP/1.1 {status_line}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n");
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.shutdown().await;
        }
    });
    format!("http://127.0.0.1:{port}")
}

async fn probe(base: String) -> PublicReadProbe {
    let storage: Arc<dyn StorageService> = Arc::new(StubStorage { base });
    probe_public_read(storage.as_ref()).await
}

#[tokio::test]
async fn a_404_means_anonymous_reads_are_open() {
    // The object is missing — of course it is, the key is a sentinel. What
    // matters is that the store was willing to say so.
    let base = stub_origin("404 Not Found").await;
    assert!(matches!(probe(base).await, PublicReadProbe::Readable));
}

#[tokio::test]
async fn a_403_means_every_image_on_the_site_will_be_broken() {
    // An object store refuses to reveal whether an object exists to a caller who
    // may not read it. This is the response a visitor's browser gets for every
    // avatar, logo and post image.
    let base = stub_origin("403 Forbidden").await;
    assert!(matches!(probe(base).await, PublicReadProbe::Forbidden));
}

#[tokio::test]
async fn a_401_is_treated_the_same_as_a_403() {
    let base = stub_origin("401 Unauthorized").await;
    assert!(matches!(probe(base).await, PublicReadProbe::Forbidden));
}

#[tokio::test]
async fn a_200_also_proves_reads_are_open() {
    // Someone really stored an object under the sentinel name. Unlikely, and
    // equally conclusive.
    let base = stub_origin("200 OK").await;
    assert!(matches!(probe(base).await, PublicReadProbe::Readable));
}

#[tokio::test]
async fn an_unexpected_status_is_not_reported_either_way() {
    // A 500 from a CDN says nothing about bucket permissions, and claiming it
    // does would send an operator to fix the wrong thing.
    let base = stub_origin("500 Internal Server Error").await;
    assert!(matches!(probe(base).await, PublicReadProbe::Inconclusive(_)));
}

#[tokio::test]
async fn an_unreachable_origin_is_inconclusive_not_forbidden() {
    // The probe races the HTTP listener when a CDN is pointed back at this app,
    // so a connection failure must stay quiet rather than raise a false alarm.
    // Port 1 on loopback has nothing listening.
    let probe = probe("http://127.0.0.1:1".to_string()).await;
    assert!(matches!(probe, PublicReadProbe::Inconclusive(_)));
}

#[tokio::test]
async fn a_relative_public_url_is_not_probed_at_all() {
    // Database storage without a CDN: this process serves the bytes, so there is
    // no third party whose permissions could be wrong and no request to make.
    let storage: Arc<dyn StorageService> = Arc::new(StubStorage {
        base: "/files".to_string(),
    });
    assert!(matches!(
        probe_public_read(storage.as_ref()).await,
        PublicReadProbe::SameOrigin
    ));
}
