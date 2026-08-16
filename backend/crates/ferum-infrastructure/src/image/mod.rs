//! Upload image pipeline: decode, re-frame, re-encode.
//!
//! Two properties of this module are load-bearing and neither is visible from a
//! call site.
//!
//! **Everything runs off the async runtime.** Encoding a 12 MP photo is several
//! hundred milliseconds of solid CPU. Left on a tokio worker it stalls every
//! other request sharing that thread, which is how an upload feature turns into
//! a site-wide latency regression that profiling blames on the wrong handler.
//!
//! **Concurrency is capped by a semaphore, acquired before the bytes are
//! touched.** Peak memory is `permits × IMAGE_MAX_ALLOC_BYTES`; without the cap
//! it is bounded only by how many people upload at once.

mod decode;
mod encode;
/// `pub` for one reason: `clamp_crop` is the arithmetic that turns an
/// attacker-chosen rectangle into a safe one, and its failure mode is a
/// *plausible* wrong answer rather than a crash. Reaching it only through
/// `process` would mean testing overflow behaviour by round-tripping real JPEGs.
pub mod frame;

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use bytes::Bytes;
use ferum_application::ports::{CropRect, ImagePolicy, ImageProcessor, ProcessedImage};
use ferum_application::shared::AppError;
use tokio::sync::Semaphore;

/// The working image pipeline, selected when the `image_processing` feature is
/// compiled in.
pub struct RealImageProcessor {
    permits: Arc<Semaphore>,
}

impl RealImageProcessor {
    /// Sizes the concurrency cap at one below the available parallelism, floored
    /// at 1.
    ///
    /// Leaving a core free is deliberate: image work is the only genuinely
    /// CPU-bound thing this process does, and saturating every core makes the
    /// HTTP runtime itself unschedulable — page renders start queueing behind
    /// avatar uploads.
    pub fn new() -> Self {
        let permits = std::thread::available_parallelism()
            .map(|n| n.get().saturating_sub(1).max(1))
            .unwrap_or(1);
        tracing::info!(permits, "image processing enabled");
        Self {
            permits: Arc::new(Semaphore::new(permits)),
        }
    }
}

impl Default for RealImageProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ImageProcessor for RealImageProcessor {
    async fn process(
        &self,
        data: Bytes,
        content_type: &str,
        policy: ImagePolicy,
        crop: Option<CropRect>,
    ) -> Result<ProcessedImage, AppError> {
        // Before the bytes are read, not after: the permit is what bounds peak
        // memory, so decoding first would let an arbitrary number of uploads
        // allocate simultaneously and only then queue.
        let _permit = self
            .permits
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| AppError::internal("image processor unavailable"))?;

        let bytes_in = data.len();
        let owned_type = content_type.to_string();
        let started = Instant::now();

        let outcome =
            tokio::task::spawn_blocking(move || run(data, &owned_type, policy, crop)).await;

        let duration_ms = started.elapsed().as_millis();
        match outcome {
            Ok(Ok(processed)) => {
                tracing::info!(
                    bytes_in,
                    bytes_out = processed.data.len(),
                    // Integer percent rather than a float: this is read in log
                    // aggregation, where a ratio of `0.23178` is noise.
                    percent_of_original = processed.data.len() * 100 / bytes_in.max(1),
                    duration_ms,
                    content_type = %processed.content_type,
                    "image processed"
                );
                Ok(processed)
            }
            Ok(Err(err)) => {
                tracing::warn!(
                    bytes_in,
                    duration_ms,
                    code = err.status_and_code().1,
                    "image rejected"
                );
                Err(err)
            }
            // A decoder panic is malformed input reaching a path that did not
            // expect it. It must become a 4xx, never a downed worker: the input
            // came from an anonymous uploader, so propagating it would hand out
            // a remote crash.
            Err(join_err) => {
                tracing::warn!(
                    bytes_in,
                    duration_ms,
                    panicked = join_err.is_panic(),
                    "image processing task did not complete"
                );
                Err(AppError::invalid("image_decode_failed"))
            }
        }
    }
}

/// Size below which an already-conforming image is stored without decoding.
///
/// The trade this makes, stated plainly: a 299 KB photo that *would* have
/// compressed to 150 KB is stored at 299 KB, buying back the ~200 ms of CPU the
/// decode-and-encode would have cost. That is the right way round on the
/// hardware this runs on — a shared-core VM where sustained image work exhausts
/// burst capacity — and the loss is bounded by this constant by construction.
const SKIP_BELOW_BYTES: usize = 300 * 1024;

/// Whether the upload can be stored exactly as received, without decoding it.
///
/// Every condition here is answered from the file's **header**, so the check
/// costs microseconds against the hundreds of milliseconds a decode does. It is
/// deliberately conservative: any doubt returns `false` and the full pipeline
/// runs.
fn can_store_unchanged(data: &[u8], policy: ImagePolicy, crop: Option<CropRect>) -> bool {
    // A crop rectangle is a request to change the image; there is no version of
    // honouring it that skips the work.
    if crop.is_some() || data.len() >= SKIP_BELOW_BYTES {
        return false;
    }

    // The source must already be the format this policy emits. Skipping a PNG
    // under `Preserve` would store a PNG where every other upload on that path
    // is a JPEG — not wrong, but it makes the stored format depend on what the
    // user happened to pick, which is the kind of inconsistency that surfaces
    // much later as "why is this one file different".
    //
    // `FixedFrame` is absent on purpose: it crops to an exact size, so no input
    // is ever already correct.
    let max_long_edge = match (policy, imagesize::image_type(data)) {
        (ImagePolicy::Preserve { max_long_edge, .. }, Ok(imagesize::ImageType::Jpeg)) => {
            max_long_edge
        }
        (ImagePolicy::LosslessOnly { max_long_edge }, Ok(imagesize::ImageType::Png)) => {
            max_long_edge
        }
        _ => return false,
    };

    let Ok(size) = imagesize::blob_size(data) else {
        return false;
    };
    if size.width.max(size.height) as u32 > max_long_edge {
        return false;
    }

    // The last condition, and the one that must not be dropped: EXIF is where a
    // phone writes GPS coordinates, `/files/` is public, and a re-encode is the
    // only thing that removes them. A file carrying one is never fast-pathed,
    // however small.
    !decode::has_exif(data)
}

/// The synchronous pipeline. Runs inside `spawn_blocking`.
fn run(
    data: Bytes,
    content_type: &str,
    policy: ImagePolicy,
    crop: Option<CropRect>,
) -> Result<ProcessedImage, AppError> {
    // Cheapest exit first. On a forum whose uploads are mostly small,
    // already-optimised images this is the common case, and taking it turns
    // ~200 ms of decode-and-encode into a header read.
    if can_store_unchanged(&data, policy, crop) {
        tracing::debug!(
            bytes = data.len(),
            "already within policy — stored without decoding"
        );
        let content_type = content_type_of(content_type, &data);
        return Ok(ProcessedImage { data, content_type });
    }

    // Nothing here can encode an animation, so processing a multi-frame GIF
    // would return a single still — a valid image, silently missing the whole
    // point of the upload. Hand it back exactly as it arrived instead.
    if decode::is_animated_gif(&data) {
        return Ok(ProcessedImage {
            data,
            content_type: content_type.to_string(),
        });
    }

    decode::preflight(&data)?;
    // Read before decoding: re-encoding drops metadata, so this is the last
    // moment the rotation is knowable.
    let orientation = decode::orientation(&data);
    let img = decode::apply_orientation(decode::decode(&data)?, orientation);
    let (source_w, source_h) = (img.width(), img.height());

    let img = match crop {
        Some(rect) => {
            let safe = frame::clamp_crop(rect, img.width(), img.height())?;
            img.crop_imm(safe.x, safe.y, safe.w, safe.h)
        }
        None => img,
    };

    let (bytes, content_type) = match policy {
        ImagePolicy::FixedFrame {
            width,
            height,
            quality,
        } => encode::to_jpeg(&frame::fit_frame(&img, width, height)?, quality)?,
        ImagePolicy::Preserve {
            max_long_edge,
            quality,
        } => encode::to_jpeg(&frame::limit_long_edge(img, max_long_edge)?, quality)?,
        ImagePolicy::LosslessOnly { max_long_edge } => {
            encode::to_optimised_png(&frame::limit_long_edge(img, max_long_edge)?)?
        }
    };

    // ─── Never hand back more bytes than we were given ───────────────────────
    //
    // A source that is *already* compressed harder than our quality setting —
    // a q55 phone JPEG, a squeezed PNG, any flat-colour graphic that PNG
    // encodes better than JPEG ever will — comes out of a re-encode LARGER.
    // Measured: a 1.48 MB q55 photo became 2.21 MB, a 19 KB screenshot became
    // 39 KB. The pipeline reported success both times.
    //
    // Only applies when nothing about the image actually had to change. A crop,
    // a rotation or a resize alters what the file depicts, and correctness
    // there outranks size — the output is the point, whatever it costs.
    //
    // Note this can fire under `FixedFrame` too, when the source already *is*
    // the frame: the dimensions match, so keeping the smaller original is both
    // correct and better. That is why the comparison is against the decoded
    // dimensions rather than the policy.
    let geometry_unchanged = crop.is_none()
        && orientation == 1
        && dimensions_of(&bytes).is_some_and(|(w, h)| w == source_w && h == source_h);

    // EXIF is the one thing a re-encode buys besides bytes: it is where a phone
    // writes GPS coordinates, and `/files/` is public. A file carrying one is
    // worth re-encoding even when that grows it; a file without has nothing to
    // strip, so the smaller original wins.
    if geometry_unchanged && bytes.len() >= data.len() && !decode::has_exif(&data) {
        tracing::debug!(
            bytes_in = data.len(),
            bytes_out = bytes.len(),
            "re-encode would grow the file — keeping the original"
        );
        let content_type = content_type_of(&content_type, &data);
        return Ok(ProcessedImage { data, content_type });
    }

    Ok(ProcessedImage {
        data: Bytes::from(bytes),
        content_type,
    })
}

/// Dimensions of encoded bytes, read from the header.
fn dimensions_of(data: &[u8]) -> Option<(u32, u32)> {
    imagesize::blob_size(data)
        .ok()
        .map(|s| (s.width as u32, s.height as u32))
}

/// The type describing bytes we are handing back **unmodified**.
///
/// Derived from the bytes rather than echoed from the caller: the declared type
/// is client-supplied and only its *family* was validated upstream, while
/// `cas_key` turns whatever this returns into the stored file's extension.
/// Falling back to the encoder's answer keeps that honest when the payload is
/// unrecognisable, which `preflight` has already made unlikely.
fn content_type_of(fallback: &str, data: &[u8]) -> String {
    match imagesize::image_type(data) {
        Ok(imagesize::ImageType::Jpeg) => "image/jpeg".to_string(),
        Ok(imagesize::ImageType::Png) => "image/png".to_string(),
        Ok(imagesize::ImageType::Webp) => "image/webp".to_string(),
        Ok(imagesize::ImageType::Gif) => "image/gif".to_string(),
        _ => fallback.to_string(),
    }
}
