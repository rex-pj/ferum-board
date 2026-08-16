//! Getting untrusted bytes into a `DynamicImage` without letting them decide
//! how much memory that costs.
//!
//! Every function here runs on data an anonymous uploader controls completely.
//! The ordering in [`super::RealImageProcessor`] depends on `preflight` being
//! cheap enough to run first and strict enough to matter.

use std::io::Cursor;

use ferum_application::constants::{IMAGE_MAX_ALLOC_BYTES, MAX_UPLOAD_MEGAPIXELS};
use ferum_application::shared::AppError;
use image::{DynamicImage, ImageReader};

/// Backstop against an absurd single dimension. The real size bound is
/// [`MAX_UPLOAD_MEGAPIXELS`] — this only catches a shape no legitimate upload
/// has, and is set high enough that a wide panorama within the pixel budget
/// still passes.
const MAX_DIMENSION: u32 = 20_000;

/// Rejects an oversized image from its header, before a pixel buffer exists.
///
/// This is the check that matters. `MAX_*_BYTES` bounds the compressed file and
/// says nothing about what it expands to: a valid 400 KB PNG can declare
/// 50000×50000 and ask the decoder for roughly 10 GB.
///
/// # Errors
/// `image_too_many_pixels` past the ceiling, `image_decode_failed` when no
/// header can be parsed at all.
pub fn preflight(data: &[u8]) -> Result<(), AppError> {
    let size = imagesize::blob_size(data).map_err(|_| AppError::invalid("image_decode_failed"))?;
    let pixels = (size.width as u64).saturating_mul(size.height as u64);
    if pixels > u64::from(MAX_UPLOAD_MEGAPIXELS) * 1_000_000 {
        return Err(AppError::invalid_with(
            "image_too_many_pixels",
            [("max_mp", MAX_UPLOAD_MEGAPIXELS.into())],
        ));
    }
    Ok(())
}

/// Decodes under an allocation ceiling.
///
/// `preflight` has already bounded the pixel count from the header; this bounds
/// the decoder itself, for the case where the header and the payload disagree.
///
/// # Errors
/// `image_decode_failed` on malformed input or a limit overrun. The underlying
/// message is deliberately dropped — decoder errors describe file internals and
/// are of no use to the person who picked the wrong file.
pub fn decode(data: &[u8]) -> Result<DynamicImage, AppError> {
    let mut reader = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(|_| AppError::invalid("image_decode_failed"))?;

    let mut limits = image::Limits::default();
    limits.max_alloc = Some(IMAGE_MAX_ALLOC_BYTES);
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    reader.limits(limits);

    reader
        .decode()
        .map_err(|_| AppError::invalid("image_decode_failed"))
}

/// Reads the EXIF orientation tag, defaulting to 1 (upright) whenever it cannot
/// be determined.
///
/// Must be read *before* re-encoding, which drops all metadata. Skipping it is
/// why re-encoded phone photos come out sideways: the pixels were never rotated,
/// the viewer was just being told to.
pub fn orientation(data: &[u8]) -> u32 {
    let mut cursor = Cursor::new(data);
    let Ok(exif) = exif::Reader::new().read_from_container(&mut cursor) else {
        return 1;
    };
    exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)
        .and_then(|field| field.value.get_uint(0))
        .unwrap_or(1)
}

/// Whether the file carries an EXIF block at all.
///
/// Used to decide whether re-encoding is worth paying for when it does not save
/// space: EXIF is where a phone writes GPS coordinates, and this application
/// serves uploads at a public URL. A file that has none has nothing to strip,
/// so the original bytes can be kept; one that does is worth re-encoding even
/// if the result is larger.
pub fn has_exif(data: &[u8]) -> bool {
    let mut cursor = Cursor::new(data);
    exif::Reader::new().read_from_container(&mut cursor).is_ok()
}

/// Bakes an EXIF orientation into the pixels.
///
/// The odd-numbered mirrored cases (5 and 7) are genuinely rare but cost one
/// line each, and getting them wrong produces a mirrored photograph — visually
/// plausible and therefore easy to ship unnoticed.
pub fn apply_orientation(img: DynamicImage, orientation: u32) -> DynamicImage {
    match orientation {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),
        6 => img.rotate90(),
        7 => img.rotate270().fliph(),
        8 => img.rotate270(),
        _ => img,
    }
}

/// True when the payload is a GIF carrying more than one frame.
///
/// Such a file must bypass the pipeline entirely: nothing here can encode an
/// animation, so processing it would silently return a single still frame — a
/// valid image, an unreported loss, and a support ticket that reads "my GIF
/// stopped moving".
///
/// Decoding stops after the second frame, so the cost is bounded regardless of
/// how many the file actually holds.
pub fn is_animated_gif(data: &[u8]) -> bool {
    use image::AnimationDecoder;

    let Ok(decoder) = image::codecs::gif::GifDecoder::new(Cursor::new(data)) else {
        return false;
    };
    decoder.into_frames().take(2).count() > 1
}
