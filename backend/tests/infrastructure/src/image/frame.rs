//! `clamp_crop` — the arithmetic that turns a client-chosen rectangle into one
//! that is guaranteed to sit inside the decoded image.
//!
//! Every case here fails *quietly* if the code is wrong: a wrapped addition
//! produces a valid-looking crop of the wrong region, not a panic.

use ferum_application::ports::CropRect;
use ferum_infrastructure::image::frame::clamp_crop;

fn rect(x: u32, y: u32, w: u32, h: u32) -> CropRect {
    CropRect { x, y, w, h }
}

#[test]
fn a_rect_already_inside_the_image_is_returned_unchanged() {
    let out = clamp_crop(rect(10, 20, 100, 50), 400, 300).expect("in-bounds rect is valid");
    assert_eq!((out.x, out.y, out.w, out.h), (10, 20, 100, 50));
}

#[test]
fn a_rect_running_past_the_edges_is_truncated_not_shifted() {
    // Truncating keeps the origin the user chose; shifting the rect back inside
    // would silently crop a region they never selected.
    let out = clamp_crop(rect(350, 250, 100, 100), 400, 300).expect("clamps into range");
    assert_eq!((out.x, out.y), (350, 250));
    assert_eq!((out.w, out.h), (50, 50));
}

#[test]
fn an_origin_beyond_the_image_is_pulled_to_the_last_pixel() {
    let out = clamp_crop(rect(9_000, 9_000, 10, 10), 400, 300).expect("still yields a pixel");
    assert_eq!((out.x, out.y), (399, 299));
    assert_eq!((out.w, out.h), (1, 1));
}

#[test]
fn a_width_that_overflows_the_right_edge_is_rejected_rather_than_wrapped() {
    // `x + w` wrapping produces a small, entirely plausible right edge. Without
    // `checked_add` this returns Ok with a crop of the wrong region.
    let err = clamp_crop(rect(50, 50, u32::MAX, 10), 400, 300)
        .expect_err("overflowing width must not wrap");
    assert_eq!(err.status_and_code().1, "image_crop_invalid");
}

#[test]
fn a_height_that_overflows_the_bottom_edge_is_rejected() {
    let err = clamp_crop(rect(50, 50, 10, u32::MAX), 400, 300)
        .expect_err("overflowing height must not wrap");
    assert_eq!(err.status_and_code().1, "image_crop_invalid");
}

#[test]
fn a_zero_width_rect_is_rejected() {
    // `crop_imm` on a zero-area rect yields an empty image that encodes without
    // complaint, so this has to be refused here or it reaches storage.
    let err = clamp_crop(rect(10, 10, 0, 50), 400, 300).expect_err("zero width is unusable");
    assert_eq!(err.status_and_code().1, "image_crop_invalid");
}

#[test]
fn a_zero_height_rect_is_rejected() {
    let err = clamp_crop(rect(10, 10, 50, 0), 400, 300).expect_err("zero height is unusable");
    assert_eq!(err.status_and_code().1, "image_crop_invalid");
}
