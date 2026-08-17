// Everything below is fixtures and assertions, so the whole module goes away
// outside a test build. The sibling suites achieve this with `#[cfg(test)] mod`
// per file; here the shared fixture helpers live at this level too, and gating
// each one separately would be a dozen attributes saying the same thing.
#![cfg(test)]

mod bench;
mod emit_fixtures;
mod fast_path;
mod frame;
mod no_growth;
mod pipeline;

use std::io::Cursor;

use image::{DynamicImage, ImageFormat, Rgb, RgbImage, Rgba, RgbaImage};

/// A PNG whose pixels vary, so a resize has something to actually filter and a
/// flat-colour false pass is impossible.
pub fn png_bytes(width: u32, height: u32) -> Vec<u8> {
    let img = RgbImage::from_fn(width, height, |x, y| {
        Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
    });
    encode(&DynamicImage::ImageRgb8(img), ImageFormat::Png)
}

/// A PNG carrying a real alpha gradient — the logo case, where the pipeline
/// must not flatten transparency.
pub fn png_with_alpha(width: u32, height: u32) -> Vec<u8> {
    let img = RgbaImage::from_fn(width, height, |x, _| {
        Rgba([255, 0, 0, (x % 256) as u8])
    });
    encode(&DynamicImage::ImageRgba8(img), ImageFormat::Png)
}

pub fn jpeg_bytes(width: u32, height: u32) -> Vec<u8> {
    let img = RgbImage::from_fn(width, height, |x, y| {
        Rgb([(x % 256) as u8, (y % 256) as u8, 128])
    });
    encode(&DynamicImage::ImageRgb8(img), ImageFormat::Jpeg)
}

/// A two-frame GIF. The pipeline must hand this back byte-for-byte: it cannot
/// encode an animation, so "processing" it would return one still frame and
/// report success.
pub fn animated_gif() -> Vec<u8> {
    use image::codecs::gif::GifEncoder;
    use image::{Delay, Frame};

    let mut out = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut out);
        for shade in [0u8, 255u8] {
            let frame = RgbaImage::from_pixel(8, 8, Rgba([shade, shade, shade, 255]));
            encoder
                .encode_frame(Frame::from_parts(
                    frame,
                    0,
                    0,
                    Delay::from_numer_denom_ms(100, 1),
                ))
                .expect("fixture gif frame");
        }
    }
    out
}

/// A JPEG carrying an EXIF Orientation tag, spliced in by hand.
///
/// The `image` crate writes no EXIF, so there is no way to produce this with an
/// encoder — and without it the rotation path has nothing exercising it. That
/// path is what stands between a phone upload and a sideways avatar.
///
/// `orientation` follows the EXIF values: 6 means "rotate 90° clockwise".
pub fn jpeg_with_orientation(width: u32, height: u32, orientation: u16) -> Vec<u8> {
    // Offsets inside an EXIF block are relative to the start of the TIFF
    // header, which is why it is assembled separately before being measured.
    let mut tiff = Vec::new();
    tiff.extend_from_slice(b"II"); // little-endian
    tiff.extend_from_slice(&42u16.to_le_bytes());
    tiff.extend_from_slice(&8u32.to_le_bytes()); // IFD0 begins 8 bytes in
    tiff.extend_from_slice(&1u16.to_le_bytes()); // one entry
    tiff.extend_from_slice(&0x0112u16.to_le_bytes()); // Orientation
    tiff.extend_from_slice(&3u16.to_le_bytes()); // type SHORT
    tiff.extend_from_slice(&1u32.to_le_bytes()); // count
    tiff.extend_from_slice(&orientation.to_le_bytes());
    tiff.extend_from_slice(&[0, 0]); // SHORT is padded to the 4-byte value slot
    tiff.extend_from_slice(&0u32.to_le_bytes()); // no next IFD

    let mut payload = b"Exif\0\0".to_vec();
    payload.extend_from_slice(&tiff);

    let base = jpeg_bytes(width, height);
    let mut out = vec![0xFF, 0xD8]; // SOI
    out.extend_from_slice(&[0xFF, 0xE1]); // APP1
    // The length field counts itself, hence +2.
    out.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(&payload);
    out.extend_from_slice(&base[2..]); // everything after the original SOI
    out
}

/// A hand-built PNG header declaring 50000×50000 and carrying no pixel data.
///
/// This is the decompression bomb in its honest form: ~70 bytes on the wire,
/// ~7.5 GB once decoded. It cannot be produced with an encoder — that is the
/// whole point, and why the guard has to read the header rather than the size.
pub fn pixel_bomb_png() -> Vec<u8> {
    let mut data = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    data.extend_from_slice(&13u32.to_be_bytes()); // IHDR payload length
    data.extend_from_slice(b"IHDR");
    data.extend_from_slice(&50_000u32.to_be_bytes()); // width
    data.extend_from_slice(&50_000u32.to_be_bytes()); // height
    data.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit truecolour
    data.extend_from_slice(&[0, 0, 0, 0]); // CRC — unchecked by a header read
    data
}

fn encode(img: &DynamicImage, format: ImageFormat) -> Vec<u8> {
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), format)
        .expect("fixture encode");
    out
}

/// Dimensions of an encoded image, for asserting on pipeline output.
pub fn dimensions(data: &[u8]) -> (u32, u32) {
    let img = image::load_from_memory(data).expect("output should be a decodable image");
    (img.width(), img.height())
}
