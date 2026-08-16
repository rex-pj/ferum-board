//! The header-only fast path: an upload that already conforms is stored without
//! being decoded at all.
//!
//! Every test here asserts on **byte identity**, because that is the only
//! observable difference between "skipped" and "decoded and re-encoded to
//! something similar". A test that checked dimensions or content type would
//! pass either way and prove nothing.
//!
//! The conditions that must keep *disqualifying* an upload matter more than the
//! ones that admit it — each is a way the optimisation could quietly start
//! storing the wrong thing.

use bytes::Bytes;
use ferum_application::ports::{CropRect, ImagePolicy, ImageProcessor};
use ferum_infrastructure::image::RealImageProcessor;
use image::{DynamicImage, Rgb, RgbImage};

use super::{jpeg_with_orientation, png_with_alpha};

/// A PNG holding photographic detail, so JPEG genuinely beats it. `png_bytes`
/// in the shared fixtures is a smooth gradient, which PNG stores very well.
fn photographic_png(width: u32, height: u32) -> Vec<u8> {
    let img = RgbImage::from_fn(width, height, |x, y| {
        let n = (x.wrapping_mul(2_654_435_761)) ^ (y.wrapping_mul(40_503));
        Rgb([(n >> 3) as u8, (n >> 11) as u8, (n >> 19) as u8])
    });
    let mut out = Vec::new();
    DynamicImage::ImageRgb8(img)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .expect("fixture png");
    out
}

const PRESERVE: ImagePolicy = ImagePolicy::Preserve {
    max_long_edge: 2048,
    quality: 82,
};

/// A small, detailed JPEG — comfortably under the skip threshold and under the
/// long-edge cap, so it qualifies on every count.
fn small_jpeg(width: u32, height: u32) -> Vec<u8> {
    let img = RgbImage::from_fn(width, height, |x, y| {
        Rgb([(x % 200) as u8, (y % 200) as u8, ((x ^ y) % 200) as u8])
    });
    let mut out = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 70);
    encoder
        .encode(
            DynamicImage::ImageRgb8(img).to_rgb8().as_raw(),
            width,
            height,
            image::ExtendedColorType::Rgb8,
        )
        .expect("fixture jpeg");
    out
}

#[tokio::test]
async fn a_conforming_jpeg_is_returned_byte_for_byte() {
    let source = small_jpeg(800, 600);
    assert!(source.len() < 300 * 1024, "fixture must be under the threshold");

    let out = RealImageProcessor::new()
        .process(Bytes::from(source.clone()), "image/jpeg", PRESERVE, None)
        .await
        .expect("must process");

    assert_eq!(
        out.data.as_ref(),
        source.as_slice(),
        "a conforming upload must not be decoded at all"
    );
    assert_eq!(out.content_type, "image/jpeg");
}

#[tokio::test]
async fn a_conforming_png_is_returned_byte_for_byte_under_the_lossless_policy() {
    let source = png_with_alpha(300, 300);
    assert!(source.len() < 300 * 1024, "fixture must be under the threshold");

    let out = RealImageProcessor::new()
        .process(
            Bytes::from(source.clone()),
            "image/png",
            ImagePolicy::LosslessOnly { max_long_edge: 512 },
            None,
        )
        .await
        .expect("must process");

    assert_eq!(out.data.as_ref(), source.as_slice());
    assert_eq!(out.content_type, "image/png");
}

// ─── The disqualifiers ───────────────────────────────────────────────────────

#[tokio::test]
async fn a_file_carrying_exif_is_never_fast_pathed() {
    // The privacy condition. EXIF is where GPS coordinates live and `/files/`
    // is public, so however small the file, it gets re-encoded to strip them.
    // Orientation 1 so nothing rotates — the *only* reason this differs from
    // the input is the metadata.
    let source = jpeg_with_orientation(400, 300, 1);
    assert!(source.len() < 300 * 1024, "fixture must be under the threshold");

    let out = RealImageProcessor::new()
        .process(Bytes::from(source.clone()), "image/jpeg", PRESERVE, None)
        .await
        .expect("must process");

    assert_ne!(
        out.data.as_ref(),
        source.as_slice(),
        "an EXIF-carrying upload must be re-encoded even when it is small"
    );
}

#[tokio::test]
async fn a_crop_request_is_never_fast_pathed() {
    let source = small_jpeg(800, 600);

    let out = RealImageProcessor::new()
        .process(
            Bytes::from(source.clone()),
            "image/jpeg",
            PRESERVE,
            Some(CropRect { x: 10, y: 10, w: 200, h: 150 }),
        )
        .await
        .expect("must process");

    assert_ne!(out.data.as_ref(), source.as_slice());
    let (w, h) = super::dimensions(&out.data);
    assert_eq!((w, h), (200, 150), "the crop must actually be applied");
}

#[tokio::test]
async fn an_image_over_the_long_edge_cap_is_never_fast_pathed() {
    // Small in bytes, large in pixels — the combination the byte threshold
    // alone would wave through.
    let source = small_jpeg(3000, 400);

    let out = RealImageProcessor::new()
        .process(Bytes::from(source.clone()), "image/jpeg", PRESERVE, None)
        .await
        .expect("must process");

    let (w, _) = super::dimensions(&out.data);
    assert_eq!(w, 2048, "an oversized image must still be downscaled");
}

#[tokio::test]
async fn a_png_is_never_fast_pathed_under_the_lossy_policy() {
    // Format mismatch: `Preserve` emits JPEG, so skipping a PNG here would make
    // the stored format depend on what the uploader happened to pick.
    //
    // Photographic rather than flat, and deliberately: a flat PNG converts to a
    // *larger* JPEG, at which point the no-growth guard returns the original
    // PNG and this test would be measuring that instead. The property under
    // test is the format rule, so the fixture has to be one where JPEG wins.
    let source = photographic_png(600, 600);

    let out = RealImageProcessor::new()
        .process(Bytes::from(source.clone()), "image/png", PRESERVE, None)
        .await
        .expect("must process");

    assert_eq!(
        out.content_type, "image/jpeg",
        "Preserve must emit JPEG regardless of what came in"
    );
    assert_ne!(out.data.as_ref(), source.as_slice());
}

#[tokio::test]
async fn a_fixed_frame_still_reaches_its_frame_from_a_conforming_source() {
    // `FixedFrame` is excluded from the fast path, but a source that is already
    // exactly the frame can still come back untouched — the no-growth guard
    // fires when the output would be larger at identical dimensions. That is
    // correct (right frame, fewer bytes), so what this asserts is the frame,
    // not the bytes.
    let already_framed = small_jpeg(512, 512);
    let out = RealImageProcessor::new()
        .process(
            Bytes::from(already_framed),
            "image/jpeg",
            ImagePolicy::FixedFrame { width: 512, height: 512, quality: 82 },
            None,
        )
        .await
        .expect("must process");
    assert_eq!(super::dimensions(&out.data), (512, 512));

    // A source that is *not* already the frame must be re-encoded into it —
    // the case that proves the exclusion is doing something.
    let wrong_shape = small_jpeg(900, 300);
    let out = RealImageProcessor::new()
        .process(
            Bytes::from(wrong_shape.clone()),
            "image/jpeg",
            ImagePolicy::FixedFrame { width: 512, height: 512, quality: 82 },
            None,
        )
        .await
        .expect("must process");
    assert_eq!(super::dimensions(&out.data), (512, 512));
    assert_ne!(out.data.as_ref(), wrong_shape.as_slice());
}
