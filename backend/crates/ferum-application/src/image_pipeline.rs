//! Chooses the [`ImagePolicy`] for an upload and applies it.
//!
//! This exists so the seven upload paths inject **one** thing instead of a
//! processor plus a config repository plus a copy of the feature→policy
//! mapping. Two consequences follow from that, and both are the point:
//!
//! * The admin toggle is honoured in exactly one place. Seven independent reads
//!   of `image_processing_enabled` is seven chances for one path to keep
//!   re-encoding after an admin turned it off.
//! * Which upload may be cropped is decided by a table here rather than at the
//!   call sites, so "post attachments are never cropped" is a line of code
//!   somebody can read, not a convention spread over six files.

use std::sync::Arc;

use bytes::Bytes;

use crate::constants::{
    AVATAR_FRAME, COVER_FRAME, DEFAULT_IMAGE_JPEG_QUALITY, DEFAULT_IMAGE_MAX_LONG_EDGE,
    LOGO_MAX_LONG_EDGE, THEME_PREVIEW_MAX_LONG_EDGE, THUMBNAIL_FRAME,
};
use crate::ports::{CropRect, ImagePolicy, ImageProcessor, ProcessedImage};
use crate::shared::AppError;
use ferum_domain::repositories::site_config_repository::{get_config_u64, SiteConfigRepository};

/// Which upload path an image arrived on.
///
/// The variant, not the caller, decides whether cropping is allowed — see
/// [`ImageTarget::policy`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageTarget {
    Avatar,
    Cover,
    Thumbnail,
    Attachment,
    ProductMedia,
    PluginMedia,
    Logo,
    ThemePreview,
}

impl ImageTarget {
    /// Maps a target to the treatment its layout justifies.
    ///
    /// The split is between images the *layout* owns and images the *user* owns.
    /// An avatar has to be square because the frame around it is; an attachment
    /// is whatever its author chose to show, and re-framing it server-side
    /// throws away the part they meant.
    fn policy(self, quality: u8, max_long_edge: u32) -> ImagePolicy {
        match self {
            Self::Avatar => ImagePolicy::FixedFrame {
                width: AVATAR_FRAME.0,
                height: AVATAR_FRAME.1,
                quality,
            },
            Self::Cover => ImagePolicy::FixedFrame {
                width: COVER_FRAME.0,
                height: COVER_FRAME.1,
                quality,
            },
            Self::Thumbnail => ImagePolicy::FixedFrame {
                width: THUMBNAIL_FRAME.0,
                height: THUMBNAIL_FRAME.1,
                quality,
            },
            Self::Attachment | Self::ProductMedia | Self::PluginMedia => ImagePolicy::Preserve {
                max_long_edge,
                quality,
            },
            Self::ThemePreview => ImagePolicy::Preserve {
                max_long_edge: THEME_PREVIEW_MAX_LONG_EDGE,
                quality,
            },
            // The one target that never loses a pixel. A logo is alpha, flat
            // colour and thin text; lossy compression haloes all three.
            Self::Logo => ImagePolicy::LosslessOnly {
                max_long_edge: LOGO_MAX_LONG_EDGE,
            },
        }
    }

    /// Whether a client-supplied crop rectangle is honoured for this target.
    ///
    /// **A rectangle sent for any other target is discarded, not rejected.** A
    /// stale client that keeps sending one should not start failing uploads; it
    /// should stop having an effect.
    fn accepts_crop(self) -> bool {
        matches!(
            self,
            Self::Avatar | Self::Cover | Self::Thumbnail | Self::ProductMedia
        )
    }
}

/// Runs `pipeline` if one is configured, otherwise hands the bytes back.
///
/// Returns the pair every caller needs together: the bytes and **the content
/// type that now describes them**. Taking only the bytes and reusing the
/// caller's own `content_type` is the mistake this signature exists to prevent
/// — `cas_key` builds the stored extension from it, so a `.png` key over JPEG
/// data results, and `key_from_url` still matches it, so nothing reports it.
///
/// # Errors
/// Propagates the processor's rejections unchanged.
pub async fn apply(
    pipeline: Option<&Arc<ImagePipeline>>,
    data: Bytes,
    content_type: String,
    target: ImageTarget,
    crop: Option<CropRect>,
) -> Result<(Bytes, String), AppError> {
    let Some(pipeline) = pipeline else {
        return Ok((data, content_type));
    };
    let processed = pipeline.process(data, &content_type, target, crop).await?;
    Ok((processed.data, processed.content_type))
}

/// The upload image pipeline, as the use cases see it.
pub struct ImagePipeline {
    processor: Arc<dyn ImageProcessor>,
    site_config: Arc<dyn SiteConfigRepository>,
}

impl ImagePipeline {
    pub fn new(
        processor: Arc<dyn ImageProcessor>,
        site_config: Arc<dyn SiteConfigRepository>,
    ) -> Self {
        Self {
            processor,
            site_config,
        }
    }

    /// Applies the policy for `target`, or returns the input untouched when an
    /// admin has turned processing off.
    ///
    /// # Errors
    /// Whatever [`ImageProcessor::process`] raises — `image_too_many_pixels`,
    /// `image_decode_failed`, `image_crop_invalid`.
    pub async fn process(
        &self,
        data: Bytes,
        content_type: &str,
        target: ImageTarget,
        crop: Option<CropRect>,
    ) -> Result<ProcessedImage, AppError> {
        if !self.enabled().await {
            return Ok(ProcessedImage {
                data,
                content_type: content_type.to_string(),
            });
        }

        let quality = self.jpeg_quality().await;
        let max_long_edge = self.max_long_edge().await;
        let crop = crop.filter(|_| target.accepts_crop());

        self.processor
            .process(data, content_type, target.policy(quality, max_long_edge), crop)
            .await
    }

    /// Absent means enabled: the seeder writes `"true"`, so a missing row is an
    /// install that predates the key rather than an operator opting out.
    async fn enabled(&self) -> bool {
        self.site_config
            .get("image_processing_enabled")
            .await
            .ok()
            .flatten()
            .map(|v| v == "true")
            .unwrap_or(true)
    }

    /// Clamped rather than validated: an admin who types 500 gets a file that is
    /// enormous but correct, and one who types 0 would otherwise get a policy
    /// that divides by it.
    async fn jpeg_quality(&self) -> u8 {
        get_config_u64(
            self.site_config.as_ref(),
            "image_jpeg_quality",
            u64::from(DEFAULT_IMAGE_JPEG_QUALITY),
        )
        .await
        .clamp(40, 100) as u8
    }

    /// The floor exists for the same reason: a long edge of 0 would resize every
    /// upload to a single pixel, which no admin means and nothing else catches.
    async fn max_long_edge(&self) -> u32 {
        get_config_u64(
            self.site_config.as_ref(),
            "image_max_long_edge",
            u64::from(DEFAULT_IMAGE_MAX_LONG_EDGE),
        )
        .await
        .clamp(320, 8192) as u32
    }
}
