//! Measurement harness for tuning `image_jpeg_quality` and for checking the
//! upload latency budget.
//!
//! Both tests are `#[ignore]`d. They are instruments, not assertions about
//! correctness: one needs a corpus this repository does not ship, and the other
//! times CPU work, which makes it useless as a CI gate and actively harmful as
//! a flaky one.
//!
//! ```powershell
//! # Quality sweep over your own images (the one that answers "what should
//! # image_jpeg_quality be?").
//! $env:FERUM_IMAGE_CORPUS = "D:\photos"
//! cargo test -p ferum-infrastructure-tests --  --ignored --nocapture quality_sweep
//!
//! # Latency, no corpus needed.
//! cargo test -p ferum-infrastructure-tests -- --ignored --nocapture throughput
//! ```

use std::path::PathBuf;
use std::time::Instant;

use bytes::Bytes;
use ferum_application::ports::{ImagePolicy, ImageProcessor};
use ferum_infrastructure::image::RealImageProcessor;

use super::{jpeg_bytes, png_bytes};

/// Qualities worth comparing. 82 is the shipped default; the neighbours bracket
/// it closely enough that the knee in the size curve is visible.
const QUALITIES: [u8; 5] = [70, 76, 82, 88, 94];

fn corpus() -> Vec<PathBuf> {
    let Ok(dir) = std::env::var("FERUM_IMAGE_CORPUS") else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        panic!("FERUM_IMAGE_CORPUS is set to {dir}, which cannot be read");
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| matches!(e.to_ascii_lowercase().as_str(), "jpg" | "jpeg" | "png" | "webp"))
                .unwrap_or(false)
        })
        .collect();
    files.sort();
    files
}

fn content_type_for(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        _ => "image/jpeg",
    }
}

fn median(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).expect("no NaN in measurements"));
    values[values.len() / 2]
}

/// Percentile by nearest-rank, which needs no interpolation rule to argue about
/// and is the right shape for a latency budget.
fn percentile(values: &mut [f64], p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).expect("no NaN in measurements"));
    let rank = ((p / 100.0) * values.len() as f64).ceil().max(1.0) as usize;
    values[rank.min(values.len()) - 1]
}

/// Sweeps `image_jpeg_quality` over a real corpus and prints what each setting
/// costs in bytes.
///
/// **Run this on the forum's own subject matter.** The right quality is not a
/// universal constant: a photography board and a support board asking about
/// screenshots want different answers, and a synthetic gradient — which is all
/// this repository could ship — compresses to nothing at every setting and
/// would recommend 70 for everyone.
#[tokio::test]
#[ignore = "needs FERUM_IMAGE_CORPUS pointing at a directory of real images"]
async fn quality_sweep() {
    let files = corpus();
    assert!(
        !files.is_empty(),
        "set FERUM_IMAGE_CORPUS to a directory holding jpg/png/webp files"
    );

    let processor = RealImageProcessor::new();
    let mut totals = [0u64; QUALITIES.len()];
    let mut bytes_in_total = 0u64;
    let mut ratios: Vec<Vec<f64>> = vec![Vec::new(); QUALITIES.len()];

    println!("\n{:<34} {:>8} {:>10}", "file", "MP", "bytes in");
    for q in QUALITIES {
        print!("{:>12}", format!("q{q}"));
    }
    println!();

    for path in &files {
        let data = std::fs::read(path).expect("corpus file must be readable");
        let bytes_in = data.len() as u64;

        // Counted only for files that actually get processed. Adding it before
        // this check would inflate the denominator with skipped files and
        // report a saving larger than anything achieved.
        let (w, h) = match dimensions_of(&data) {
            Some(dim) => dim,
            None => {
                eprintln!("skipped (undecodable): {}", path.display());
                continue;
            }
        };
        bytes_in_total += bytes_in;
        let megapixels = (w as f64 * h as f64) / 1_000_000.0;

        let name = path.file_name().unwrap_or_default().to_string_lossy();
        print!("{:<34} {:>8.1} {:>10}", truncate(&name, 34), megapixels, bytes_in);

        for (i, q) in QUALITIES.iter().enumerate() {
            let out = processor
                .process(
                    Bytes::from(data.clone()),
                    content_type_for(path),
                    ImagePolicy::Preserve {
                        max_long_edge: 2048,
                        quality: *q,
                    },
                    None,
                )
                .await
                .expect("corpus image must process");
            let bytes_out = out.data.len() as u64;
            totals[i] += bytes_out;
            ratios[i].push(bytes_out as f64 / bytes_in.max(1) as f64);
            print!("{:>12}", bytes_out);
        }
        println!();
    }

    println!("\n── totals over {} files ──", files.len());
    println!("bytes in: {bytes_in_total}");
    for (i, q) in QUALITIES.iter().enumerate() {
        let saved = 100.0 - (totals[i] as f64 / bytes_in_total.max(1) as f64) * 100.0;
        println!(
            "q{q:<3} out={:<12} saved={:>5.1}%  median per-file ratio={:.3}",
            totals[i],
            saved,
            median(&mut ratios[i])
        );
    }
    println!(
        "\nPick the highest quality whose saving is still acceptable — the curve \
         flattens well before it looks better. Set it at /admin/settings → \
         Content Policy → Image Processing.\n"
    );
}

/// Times the pipeline against the budget this feature was given.
///
/// The plan set upload endpoints a **p95 < 1200 ms** budget, carved out from
/// NF-PF-04's 200 ms because encoding cannot meet it. That number was an
/// estimate; this is the measurement.
///
/// Content-independent by design: decode and encode cost scales with pixel
/// count, so synthetic images are a fair timing proxy even though they are
/// useless for `quality_sweep`.
#[tokio::test]
#[ignore = "timing measurement — informative, and too machine-dependent to gate CI"]
async fn throughput() {
    let processor = RealImageProcessor::new();

    // 1 MP ≈ a screenshot, 12 MP ≈ a current phone camera, 40 MP = the ceiling
    // `MAX_UPLOAD_MEGAPIXELS` allows through at all.
    let cases: [(&str, u32, u32); 3] = [
        ("1 MP  (1280×800 screenshot)", 1280, 800),
        ("12 MP (4000×3000 phone)", 4000, 3000),
        ("40 MP (6300×6300 ceiling)", 6300, 6300),
    ];

    println!("\n{:<32} {:>10} {:>10} {:>10} {:>12}", "case", "policy", "ms", "ms/MP", "bytes out");

    for (label, w, h) in cases {
        let jpeg = Bytes::from(jpeg_bytes(w, h));
        let png = Bytes::from(png_bytes(w, h));
        let megapixels = (w as f64 * h as f64) / 1_000_000.0;

        for (policy_name, source, policy) in [
            (
                "Preserve",
                jpeg.clone(),
                ImagePolicy::Preserve { max_long_edge: 2048, quality: 82 },
            ),
            (
                "FixedFrame",
                jpeg.clone(),
                ImagePolicy::FixedFrame { width: 512, height: 512, quality: 82 },
            ),
            (
                "Lossless",
                png.clone(),
                ImagePolicy::LosslessOnly { max_long_edge: 512 },
            ),
        ] {
            // Three runs, reporting the median: the first pays for cold caches
            // and allocator warm-up, which is not what a steady-state budget is
            // about.
            let mut samples = Vec::new();
            let mut bytes_out = 0;
            for _ in 0..3 {
                let started = Instant::now();
                let out = processor
                    .process(source.clone(), "image/jpeg", policy, None)
                    .await
                    .expect("synthetic image must process");
                samples.push(started.elapsed().as_secs_f64() * 1000.0);
                bytes_out = out.data.len();
            }
            let ms = median(&mut samples);
            println!(
                "{:<32} {:>10} {:>10.0} {:>10.1} {:>12}",
                label,
                policy_name,
                ms,
                ms / megapixels,
                bytes_out
            );
        }
    }

    // The realistic worst case an ordinary user produces, measured rather than
    // asserted from a table: a 12 MP phone photo through the attachment path.
    let phone = Bytes::from(jpeg_bytes(4000, 3000));
    let mut samples = Vec::new();
    for _ in 0..7 {
        let started = Instant::now();
        processor
            .process(
                phone.clone(),
                "image/jpeg",
                ImagePolicy::Preserve { max_long_edge: 2048, quality: 82 },
                None,
            )
            .await
            .expect("phone photo must process");
        samples.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    let p95 = percentile(&mut samples, 95.0);
    println!("\n12 MP attachment p95 = {p95:.0} ms  (documented budget: 1200 ms)");
    println!(
        "Exceeding it on your hardware means lowering image_max_long_edge, not \
         raising the budget — the number exists to bound how long a request can \
         hold a processing permit.\n"
    );
}

/// Header-only dimensions, so a 40 MP file costs nothing to size.
fn dimensions_of(data: &[u8]) -> Option<(u32, u32)> {
    let img = image::ImageReader::new(std::io::Cursor::new(data))
        .with_guessed_format()
        .ok()?;
    img.into_dimensions().ok()
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max - 1).collect::<String>() + "…"
}
