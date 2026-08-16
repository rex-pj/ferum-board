//! The pipeline must never hand back more bytes than it was given.
//!
//! This is the property a user checks first and the one the rest of the suite
//! missed: every other test here feeds synthetic gradients, which compress so
//! well that no setting makes them grow. Real uploads are already compressed —
//! a phone JPEG at q70, a PNG someone ran through a squeezer — and re-encoding
//! those at a *higher* quality is a straightforward way to make a file bigger
//! while reporting success.

use bytes::Bytes;
use ferum_application::ports::{ImagePolicy, ImageProcessor};
use ferum_infrastructure::image::RealImageProcessor;
use image::{DynamicImage, Rgb, RgbImage};
use std::io::Cursor;

/// A photo-like image with enough detail that JPEG cannot cheat on it, encoded
/// at a **low** quality so the result is already small.
///
/// This is what an upload actually looks like. `jpeg_bytes` in the shared
/// fixtures encodes at the `image` crate's default and produces a much softer
/// target.
fn already_compressed_jpeg(width: u32, height: u32, quality: u8) -> Vec<u8> {
    // Deterministic pseudo-noise: a smooth gradient compresses to almost
    // nothing and would hide the very effect under test.
    let img = RgbImage::from_fn(width, height, |x, y| {
        let n = (x.wrapping_mul(2_654_435_761)) ^ (y.wrapping_mul(40_503));
        Rgb([(n >> 3) as u8, (n >> 11) as u8, (n >> 19) as u8])
    });
    let mut out = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality);
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

/// A PNG of flat blocks — a screenshot, a diagram, a logo. PNG is very good at
/// this and JPEG is very bad at it.
fn flat_png(width: u32, height: u32) -> Vec<u8> {
    let img = RgbImage::from_fn(width, height, |x, y| {
        if (x / 64 + y / 64) % 2 == 0 {
            Rgb([250, 250, 250])
        } else {
            Rgb([20, 40, 160])
        }
    });
    let mut out = Vec::new();
    DynamicImage::ImageRgb8(img)
        .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
        .expect("fixture png");
    out
}

#[tokio::test]
async fn an_already_compressed_photo_does_not_grow() {
    // Under the long-edge cap, so nothing is resized and the only thing the
    // pipeline does is re-encode — at a *higher* quality than the source.
    let source = already_compressed_jpeg(1600, 1200, 55);

    let out = RealImageProcessor::new()
        .process(
            Bytes::from(source.clone()),
            "image/jpeg",
            ImagePolicy::Preserve {
                max_long_edge: 2048,
                quality: 82,
            },
            None,
        )
        .await
        .expect("photo must process");

    assert!(
        out.data.len() <= source.len(),
        "pipeline grew the file: {} bytes in, {} bytes out — re-encoding a \
         q55 source at q82 costs bytes and buys nothing",
        source.len(),
        out.data.len()
    );
}

#[tokio::test]
async fn a_flat_png_is_not_turned_into_a_bigger_jpeg() {
    // The screenshot case. JPEG rings around hard colour boundaries, so it is
    // both larger *and* worse here.
    let source = flat_png(1200, 900);

    let out = RealImageProcessor::new()
        .process(
            Bytes::from(source.clone()),
            "image/png",
            ImagePolicy::Preserve {
                max_long_edge: 2048,
                quality: 82,
            },
            None,
        )
        .await
        .expect("screenshot must process");

    assert!(
        out.data.len() <= source.len(),
        "pipeline grew the file: {} bytes in, {} bytes out",
        source.len(),
        out.data.len()
    );
}

#[tokio::test]
async fn a_logo_that_is_already_optimised_is_left_alone() {
    // `to_optimised_png` compares its oxipng output against its own
    // intermediate re-encode, not against what was uploaded — so a source that
    // was already squeezed can come back larger and the comparison never
    // notices.
    let source = flat_png(400, 400);

    let out = RealImageProcessor::new()
        .process(
            Bytes::from(source.clone()),
            "image/png",
            ImagePolicy::LosslessOnly { max_long_edge: 512 },
            None,
        )
        .await
        .expect("logo must process");

    assert!(
        out.data.len() <= source.len(),
        "pipeline grew the logo: {} bytes in, {} bytes out",
        source.len(),
        out.data.len()
    );
}

#[tokio::test]
async fn a_genuinely_oversized_photo_still_shrinks() {
    // The guard must not become "never do anything". A 12 MP source over the
    // cap has to come back dramatically smaller, or the feature is off.
    let source = already_compressed_jpeg(4000, 3000, 90);

    let out = RealImageProcessor::new()
        .process(
            Bytes::from(source.clone()),
            "image/jpeg",
            ImagePolicy::Preserve {
                max_long_edge: 2048,
                quality: 82,
            },
            None,
        )
        .await
        .expect("oversized photo must process");

    assert!(
        out.data.len() < source.len() / 2,
        "an oversized photo must still be cut substantially: {} → {}",
        source.len(),
        out.data.len()
    );
}
