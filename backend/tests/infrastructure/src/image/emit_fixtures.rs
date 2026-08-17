//! Writes the end-to-end verification fixtures to disk.
//!
//! Not a test — a generator, `#[ignore]`d so it never runs in CI. It exists so
//! the manual walkthrough in `docs/configuration.md` uses the *same* fixtures the
//! automated tests do, rather than whatever photo happened to be on someone's
//! desktop. A manual check against a different input is not checking the same
//! thing.
//!
//! ```powershell
//! $env:FERUM_FIXTURE_OUT = "C:\tmp\ferum-fixtures"
//! cargo test -p ferum-infrastructure-tests -- --ignored --nocapture emit_fixtures
//! ```

use std::io::Cursor;
use std::path::PathBuf;

use image::{DynamicImage, Rgb, RgbImage};

use super::{animated_gif, jpeg_with_orientation};

/// A photographic JPEG: smooth gradients with mild noise on top.
///
/// **Not pure noise.** A first attempt used it, and 12 MP came out at 22 MB —
/// incompressible by construction, over every upload limit, and nothing like a
/// camera photo. Real images have large correlated regions, which is exactly what
/// JPEG exploits, so a fixture without them measures a case that does not exist.
fn photo_jpeg(width: u32, height: u32, quality: u8) -> Vec<u8> {
    let img = RgbImage::from_fn(width, height, |x, y| {
        let fx = x as f32 / width as f32;
        let fy = y as f32 / height as f32;
        // Two smooth fields plus ±12 of dither, so fine detail exists without
        // dominating.
        let grain = ((x.wrapping_mul(2_654_435_761) ^ y.wrapping_mul(40_503)) >> 24) as i32 % 12;
        let clamp = |v: f32| (v as i32 + grain).clamp(0, 255) as u8;
        Rgb([
            clamp(220.0 * fx),
            clamp(200.0 * (1.0 - fy)),
            clamp(160.0 * ((fx + fy) * 0.5)),
        ])
    });
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality)
        .encode(
            DynamicImage::ImageRgb8(img).to_rgb8().as_raw(),
            width,
            height,
            image::ExtendedColorType::Rgb8,
        )
        .expect("fixture jpeg");
    out
}

/// A flat-colour PNG with alpha — the logo case.
fn logo_png(size: u32) -> Vec<u8> {
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_fn(size, size, |x, y| {
        let inside = (x as i32 - size as i32 / 2).pow(2) + (y as i32 - size as i32 / 2).pow(2)
            < (size as i32 / 3).pow(2);
        if inside {
            image::Rgba([20, 40, 160, 255])
        } else {
            image::Rgba([0, 0, 0, 0])
        }
    }))
    .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
    .expect("fixture png");
    out
}

#[test]
#[ignore = "generator — set FERUM_FIXTURE_OUT to a directory"]
fn emit_fixtures() {
    let dir = PathBuf::from(
        std::env::var("FERUM_FIXTURE_OUT").expect("set FERUM_FIXTURE_OUT to a directory"),
    );
    std::fs::create_dir_all(&dir).expect("create output directory");

    // 12 MP at q92: over the 2048 long-edge cap and several megabytes, so it
    // exercises both the downscale and the compression.
    let files: [(&str, Vec<u8>); 5] = [
        ("photo-12mp.jpg", photo_jpeg(4000, 3000, 92)),
        // Orientation 6 = rotate 90°. Landscape in, so upright output must be
        // portrait — visible without any metadata tooling.
        ("portrait-exif.jpg", jpeg_with_orientation(1600, 900, 6)),
        ("animated.gif", animated_gif()),
        ("logo-alpha.png", logo_png(600)),
        // Already compressed and under the cap: the no-growth guard should hand
        // this back byte-for-byte.
        ("already-small.jpg", photo_jpeg(900, 600, 55)),
    ];

    for (name, bytes) in files {
        let path = dir.join(name);
        std::fs::write(&path, &bytes).expect("write fixture");
        println!("{:<22} {:>10} bytes  {}", name, bytes.len(), path.display());
    }
    println!("\nAlso worth having: a PNG with alpha for the logo step, above.");
}
