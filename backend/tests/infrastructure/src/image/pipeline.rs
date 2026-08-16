//! `RealImageProcessor::process` end to end.
//!
//! Driven through the port rather than the internals on purpose: the guarantees
//! that matter — that a bomb is refused, that an animation survives, that the
//! returned content type matches the returned bytes — are properties of the
//! whole pipeline, and each has a way of holding inside a unit and breaking at
//! the seam.

use bytes::Bytes;
use ferum_application::ports::{CropRect, ImagePolicy, ImageProcessor};
use ferum_infrastructure::image::RealImageProcessor;

use super::{
    animated_gif, dimensions, jpeg_bytes, jpeg_with_orientation, pixel_bomb_png, png_bytes,
    png_with_alpha,
};

const FRAME: ImagePolicy = ImagePolicy::FixedFrame {
    width: 512,
    height: 512,
    quality: 82,
};
const PRESERVE: ImagePolicy = ImagePolicy::Preserve {
    max_long_edge: 2048,
    quality: 82,
};
const LOSSLESS: ImagePolicy = ImagePolicy::LosslessOnly {
    max_long_edge: 512,
};

#[tokio::test]
async fn a_fixed_frame_lands_on_exactly_the_requested_size() {
    let out = RealImageProcessor::new()
        .process(Bytes::from(png_bytes(800, 600)), "image/png", FRAME, None)
        .await
        .expect("a plain PNG should process");

    assert_eq!(dimensions(&out.data), (512, 512));
    // The content type must follow the bytes: `cas_key` builds the stored
    // extension from it, so a stale `image/png` here writes a `.png` key over
    // JPEG data — which `key_from_url` still matches, so nothing detects it.
    assert_eq!(out.content_type, "image/jpeg");
}

#[tokio::test]
async fn a_fixed_frame_crops_rather_than_squashing_a_mismatched_aspect() {
    // 2000×500 into a square: a stretch would keep all 2000 columns and make
    // everything four times too tall. A centre crop keeps the pixels square.
    let out = RealImageProcessor::new()
        .process(Bytes::from(png_bytes(2000, 500)), "image/png", FRAME, None)
        .await
        .expect("wide source should process");

    assert_eq!(dimensions(&out.data), (512, 512));
}

#[tokio::test]
async fn preserve_downscales_past_the_ceiling_and_keeps_the_aspect_ratio() {
    let out = RealImageProcessor::new()
        .process(Bytes::from(png_bytes(3000, 1500)), "image/png", PRESERVE, None)
        .await
        .expect("oversized source should process");

    let (w, h) = dimensions(&out.data);
    assert_eq!(w, 2048);
    assert_eq!(h, 1024, "2:1 in must stay 2:1 out");
}

#[tokio::test]
async fn preserve_never_enlarges_an_image_already_under_the_ceiling() {
    // Upscaling invents detail and charges bytes for it. The dimensions must
    // come back untouched.
    let out = RealImageProcessor::new()
        .process(Bytes::from(jpeg_bytes(640, 480)), "image/jpeg", PRESERVE, None)
        .await
        .expect("small source should process");

    assert_eq!(dimensions(&out.data), (640, 480));
}

#[tokio::test]
async fn lossless_only_stays_png_and_keeps_its_alpha_channel() {
    let out = RealImageProcessor::new()
        .process(
            Bytes::from(png_with_alpha(800, 800)),
            "image/png",
            LOSSLESS,
            None,
        )
        .await
        .expect("logo should process");

    assert_eq!(out.content_type, "image/png");
    assert_eq!(dimensions(&out.data), (512, 512));

    let decoded = image::load_from_memory(&out.data).expect("output decodes");
    assert!(
        decoded.color().has_alpha(),
        "a logo flattened onto an opaque background is unusable on a dark theme"
    );
}

#[tokio::test]
async fn a_header_declaring_more_pixels_than_the_ceiling_is_refused() {
    // ~70 bytes in, ~7.5 GB if decoded. Every byte-size limit in the system
    // passes this file.
    let err = RealImageProcessor::new()
        .process(Bytes::from(pixel_bomb_png()), "image/png", PRESERVE, None)
        .await
        .expect_err("a pixel bomb must not reach the decoder");

    assert_eq!(err.status_and_code().1, "image_too_many_pixels");
}

#[tokio::test]
async fn an_animated_gif_comes_back_byte_for_byte() {
    let original = animated_gif();
    let out = RealImageProcessor::new()
        .process(Bytes::from(original.clone()), "image/gif", PRESERVE, None)
        .await
        .expect("an animation should pass through");

    assert_eq!(
        out.data.as_ref(),
        original.as_slice(),
        "re-encoding an animation silently returns one still frame"
    );
    assert_eq!(out.content_type, "image/gif");
}

#[tokio::test]
async fn a_crop_is_applied_before_the_frame_is_fitted() {
    // Crop 400×400 out of a 2000×500 strip, then fit a square frame. If the
    // crop were ignored the fit would still return 512×512, so the assertion
    // that proves anything is on the pixels, not the size.
    let source = png_bytes(2000, 500);
    let processor = RealImageProcessor::new();

    let cropped = processor
        .process(
            Bytes::from(source.clone()),
            "image/png",
            FRAME,
            Some(CropRect {
                x: 0,
                y: 0,
                w: 400,
                h: 400,
            }),
        )
        .await
        .expect("cropped source should process");
    let uncropped = processor
        .process(Bytes::from(source), "image/png", FRAME, None)
        .await
        .expect("uncropped source should process");

    assert_eq!(dimensions(&cropped.data), (512, 512));
    assert_ne!(
        cropped.data, uncropped.data,
        "the crop rectangle had no effect on the output"
    );
}

#[tokio::test]
async fn an_unusable_crop_rectangle_is_reported_not_ignored() {
    let err = RealImageProcessor::new()
        .process(
            Bytes::from(png_bytes(800, 600)),
            "image/png",
            FRAME,
            Some(CropRect {
                x: 10,
                y: 10,
                w: 0,
                h: 100,
            }),
        )
        .await
        .expect_err("a zero-area crop is a client bug worth surfacing");

    assert_eq!(err.status_and_code().1, "image_crop_invalid");
}

#[tokio::test]
async fn an_exif_rotation_is_baked_into_the_pixels() {
    // Orientation 6 means "rotate 90°". Re-encoding drops EXIF, so a viewer
    // never sees the tag again — if the rotation is not applied here, a
    // portrait photo is published on its side and nothing reports it.
    let out = RealImageProcessor::new()
        .process(
            Bytes::from(jpeg_with_orientation(800, 400, 6)),
            "image/jpeg",
            PRESERVE,
            None,
        )
        .await
        .expect("an EXIF-tagged JPEG should process");

    assert_eq!(
        dimensions(&out.data),
        (400, 800),
        "a 90° rotation must swap the axes"
    );
}

#[tokio::test]
async fn an_upright_image_is_left_alone() {
    // Guards the other direction: a processor that rotates unconditionally
    // would pass the test above and turn every ordinary upload sideways.
    let out = RealImageProcessor::new()
        .process(
            Bytes::from(jpeg_with_orientation(800, 400, 1)),
            "image/jpeg",
            PRESERVE,
            None,
        )
        .await
        .expect("an upright JPEG should process");

    assert_eq!(dimensions(&out.data), (800, 400));
}

#[tokio::test]
async fn bytes_that_are_not_an_image_fail_as_a_client_error() {
    let err = RealImageProcessor::new()
        .process(
            Bytes::from_static(b"this is not an image, it is a sentence"),
            "image/png",
            PRESERVE,
            None,
        )
        .await
        .expect_err("garbage must not become a 500");

    // 4xx, not 5xx: the input was bad, the server is fine. A decoder panic
    // reaches this same code via the `JoinError` arm.
    assert_eq!(err.status_and_code().1, "image_decode_failed");
    assert!(err.status_and_code().0.is_client_error());
}
