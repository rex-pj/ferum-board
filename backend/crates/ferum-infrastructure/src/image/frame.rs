//! Crop and scale geometry.
//!
//! Isolated from the rest of the pipeline because the arithmetic here runs on
//! numbers an uploader chose, and the failure modes are silent: a wrapped
//! addition yields a *plausible* rectangle, not an obvious one.

use fast_image_resize::Resizer;
use ferum_application::ports::CropRect;
use ferum_application::shared::AppError;
use image::DynamicImage;

/// Confines a client-supplied crop to the image that was actually decoded.
///
/// The client sends what its cropper drew, which may not describe the image the
/// server holds — a different file, a stale preview, or a hand-written request.
/// The rect is therefore clamped rather than trusted.
///
/// # Errors
/// `image_crop_invalid` when the rect clamps to zero area, or when `x + w`
/// overflows — an overflow wraps to a small right edge, which reads as a
/// perfectly ordinary crop and would silently return the wrong region.
pub fn clamp_crop(rect: CropRect, width: u32, height: u32) -> Result<CropRect, AppError> {
    let x = rect.x.min(width.saturating_sub(1));
    let y = rect.y.min(height.saturating_sub(1));

    let right = x
        .checked_add(rect.w)
        .ok_or_else(|| AppError::invalid("image_crop_invalid"))?;
    let bottom = y
        .checked_add(rect.h)
        .ok_or_else(|| AppError::invalid("image_crop_invalid"))?;

    let w = right.min(width).saturating_sub(x);
    let h = bottom.min(height).saturating_sub(y);
    if w == 0 || h == 0 {
        return Err(AppError::invalid("image_crop_invalid"));
    }
    Ok(CropRect { x, y, w, h })
}

/// Centre-crops to the target aspect ratio, then scales to exactly
/// `target_w`×`target_h`.
///
/// Cropping first is what keeps the result undistorted: scaling a 3:4 portrait
/// straight into a 4:1 banner would stretch faces sideways. Centre is the only
/// defensible automatic choice — where the subject actually sits is what the
/// interactive cropper is for.
///
/// # Errors
/// `image_decode_failed` if the resize itself fails.
pub fn fit_frame(img: &DynamicImage, target_w: u32, target_h: u32) -> Result<DynamicImage, AppError> {
    let (sw, sh) = (img.width(), img.height());
    if sw == 0 || sh == 0 || target_w == 0 || target_h == 0 {
        return Err(AppError::invalid("image_decode_failed"));
    }

    // Cross-multiplied in u64: comparing `sw/sh` against `target_w/target_h` as
    // floats invites a rounding disagreement between this branch and the
    // dimension computed after it.
    let source_is_wider =
        u64::from(sw) * u64::from(target_h) > u64::from(target_w) * u64::from(sh);

    let (crop_w, crop_h) = if source_is_wider {
        let w = u64::from(sh) * u64::from(target_w) / u64::from(target_h);
        ((w as u32).clamp(1, sw), sh)
    } else {
        let h = u64::from(sw) * u64::from(target_h) / u64::from(target_w);
        (sw, (h as u32).clamp(1, sh))
    };

    let cropped = img.crop_imm((sw - crop_w) / 2, (sh - crop_h) / 2, crop_w, crop_h);
    resize_exact(&cropped, target_w, target_h)
}

/// Downscales until neither edge exceeds `max`. **Never upscales** — enlarging a
/// small image invents detail and costs bytes for it.
///
/// # Errors
/// `image_decode_failed` if the resize fails.
pub fn limit_long_edge(img: DynamicImage, max: u32) -> Result<DynamicImage, AppError> {
    let long = img.width().max(img.height());
    if max == 0 || long <= max {
        return Ok(img);
    }

    // u64 throughout: a 20000px edge times a 2048px cap overflows u32 well
    // before the division brings it back into range.
    let scale = |edge: u32| -> u32 {
        ((u64::from(edge) * u64::from(max)) / u64::from(long)).max(1) as u32
    };
    resize_exact(&img, scale(img.width()), scale(img.height()))
}

/// Scales to exactly `w`×`h` with no aspect correction.
///
/// `ResizeOptions`' defaults are what this wants and are therefore passed as
/// `None` rather than restated: Lanczos3 convolution, and `mul_div_alpha` on,
/// which premultiplies before filtering so transparent edges do not bleed the
/// colour of whatever the encoder happened to leave under them.
fn resize_exact(src: &DynamicImage, w: u32, h: u32) -> Result<DynamicImage, AppError> {
    let mut dst = DynamicImage::new(w, h, src.color());
    Resizer::new()
        .resize(src, &mut dst, None)
        .map_err(|_| AppError::invalid("image_decode_failed"))?;
    Ok(dst)
}
