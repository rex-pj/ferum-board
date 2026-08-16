//! Turning a decoded image back into bytes, and the content type that now
//! describes them.
//!
//! Both halves of the return value matter: `cas_key` derives the stored
//! extension from the content type, so returning the *input's* type after
//! changing format produces a key whose extension lies about its contents.

use ferum_application::shared::AppError;
use image::DynamicImage;
use std::io::Cursor;

/// Lossy re-encode. Used for every path whose content is a photograph.
///
/// Alpha is flattened onto white, because JPEG has no alpha channel and the
/// alternative — silently keeping PNG for any image that happens to carry one —
/// makes output size depend on a property users cannot see.
///
/// # Errors
/// `image_decode_failed` if the encoder rejects the buffer, which at this point
/// means an internal inconsistency rather than bad input.
pub fn to_jpeg(img: &DynamicImage, quality: u8) -> Result<(Vec<u8>, String), AppError> {
    let rgb = img.to_rgb8();
    let mut out = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality);
    encoder
        .encode(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|_| AppError::invalid("image_decode_failed"))?;
    Ok((out, "image/jpeg".to_string()))
}

/// Lossless re-encode, then `oxipng` over the result.
///
/// Reserved for the site logo. Lossy compression rings around the two things a
/// logo is made of — thin text and hard colour boundaries — and destroys alpha
/// outright, so this path trades bytes for the only thing that matters there.
///
/// `oxipng` failing is not fatal: the unoptimised PNG is already correct, just
/// larger, and losing an upload over a size optimisation would be the wrong
/// trade.
///
/// # Errors
/// `image_decode_failed` if the PNG encoder itself fails.
pub fn to_optimised_png(img: &DynamicImage) -> Result<(Vec<u8>, String), AppError> {
    let mut raw = Vec::new();
    img.write_to(&mut Cursor::new(&mut raw), image::ImageFormat::Png)
        .map_err(|_| AppError::invalid("image_decode_failed"))?;

    // Preset 2 is the knee of the curve: most of the reduction, none of the
    // multi-second brute-force filter search the higher presets spend it on.
    //
    // Note what this comparison is and is not. It picks the better of *our two*
    // encodes; it says nothing about the file that was uploaded, which may well
    // have been squeezed harder than either. Guarding against handing back
    // something larger than the input is `super::run`'s job, and has to be —
    // this function never sees the original bytes.
    let options = oxipng::Options::from_preset(2);
    let bytes = match oxipng::optimize_from_memory(&raw, &options) {
        Ok(optimised) if optimised.len() < raw.len() => optimised,
        Ok(_) => raw,
        Err(err) => {
            tracing::debug!(error = %err, "oxipng declined to optimise; keeping plain PNG");
            raw
        }
    };
    Ok((bytes, "image/png".to_string()))
}
