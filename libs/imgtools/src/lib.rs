pub mod error;

pub use image::DynamicImage;

use crate::error::Error;
use image::{ImageDecoder, ImageFormat, ImageReader, imageops::FilterType};
use std::io::Cursor;

/// Long side of the small image the blurred backdrop upscales from —
/// cheap smooth fill, no full-resolution gaussian blur.
const BACKDROP_LONG: u32 = 48;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Lossy JPEG, quality 90 — default for photographic sources.
    Jpeg,
    /// Lossless PNG passthrough — preserves transparency for PNG sources.
    Png,
    // Webp — future: `image`'s WebP encoder is lossless-only; lossy WebP needs libwebp.
}

/// Blurred-cover backdrop parameters for `Fit::Pad`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PadFill {
    pub blur_sigma: f32,
    pub darken: f32,
    pub desaturate: f32,
}

impl Default for PadFill {
    fn default() -> Self {
        PadFill {
            blur_sigma: 1.0,
            darken: 0.0,
            desaturate: 0.0,
        }
    }
}

/// How the source is placed into the target box.
#[derive(Debug, Clone, Copy)]
pub enum Fit {
    /// Fit inside the box (no crop), centered, padding filled with a blurred cover.
    Pad(PadFill),
    /// Cover the box and center-crop.
    Crop,
}

pub struct VariantRequest {
    pub width: u32,
    pub height: Option<u32>,
    pub aspect_ratio: Option<(u32, u32)>,
    pub fit: Fit,
    pub format: OutputFormat,
}

pub struct Variant {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub ext: &'static str,
}

/// Decode bytes into an upright, metadata-free image (the only place EXIF
/// orientation is applied).
///
/// # Errors
/// Returns an error if the bytes cannot be decoded.
pub fn decode(bytes: &[u8]) -> Result<DynamicImage, Error> {
    let reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut decoder = reader.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut img = DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    Ok(img)
}

/// Source intrinsic dimensions of an already-decoded image.
#[must_use]
pub fn dimensions(src: &DynamicImage) -> (u32, u32) {
    (src.width(), src.height())
}

/// Source **display** dimensions (EXIF orientation applied) read from the
/// container header without decoding pixels. The orientations that rotate by
/// 90°/270° swap width and height, matching what `decode` + `apply_orientation`
/// would produce.
///
/// # Errors
/// Returns an error if the bytes cannot be parsed as a known image format.
pub fn probe(bytes: &[u8]) -> Result<(u32, u32), Error> {
    let reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut decoder = reader.into_decoder()?;
    let orientation = decoder.orientation()?;
    let (w, h) = decoder.dimensions();
    Ok(match orientation {
        image::metadata::Orientation::Rotate90
        | image::metadata::Orientation::Rotate270
        | image::metadata::Orientation::Rotate90FlipH
        | image::metadata::Orientation::Rotate270FlipH => (h, w),
        _ => (w, h),
    })
}

/// Clamped output box `(width, height)` for a request against `src`.
#[must_use]
pub fn target_dimensions(src: &DynamicImage, req: &VariantRequest) -> (u32, u32) {
    target_dimensions_from(src.width(), src.height(), req)
}

/// Clamped output box from raw source dimensions, so the cache-key path can
/// size a variant without a decoded image in hand.
#[must_use]
pub fn target_dimensions_from(sw: u32, sh: u32, req: &VariantRequest) -> (u32, u32) {
    clamped_box(req, sw, sh)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn clamped_box(req: &VariantRequest, sw: u32, sh: u32) -> (u32, u32) {
    let w = req.width.max(1);
    let h = req
        .height
        .unwrap_or_else(|| {
            let (arw, arh) = req.aspect_ratio.unwrap_or((16, 9));
            (u64::from(w).saturating_mul(u64::from(arh)) / u64::from(arw.max(1))) as u32
        })
        .max(1);

    let (swf, shf, wf, hf) = (f64::from(sw), f64::from(sh), f64::from(w), f64::from(h));
    let s = match req.fit {
        // Pad fits inside: clamp so the fit scale (min ratio) stays <= 1.
        Fit::Pad(_) => 1.0_f64.min((swf / wf).max(shf / hf)),
        // Crop covers: clamp so the cover scale (max ratio) stays <= 1.
        Fit::Crop => 1.0_f64.min((swf / wf).min(shf / hf)),
    };
    let cw = (wf * s).round().max(1.0) as u32;
    let ch = (hf * s).round().max(1.0) as u32;
    (cw, ch)
}

#[allow(clippy::cast_possible_truncation)]
fn small_box(w: u32, h: u32, long: u32) -> (u32, u32) {
    let (sw, sh) = if w >= h {
        (
            long.min(w),
            (u64::from(long) * u64::from(h) / u64::from(w.max(1))) as u32,
        )
    } else {
        (
            (u64::from(long) * u64::from(w) / u64::from(h.max(1))) as u32,
            long.min(h),
        )
    };
    (sw.max(1), sh.max(1))
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::many_single_char_names
)]
fn apply_tone(buf: &mut image::RgbaImage, darken: f32, desaturate: f32) {
    let d = desaturate.clamp(0.0, 1.0);
    let k = darken.clamp(0.0, 1.0);
    for px in buf.pixels_mut() {
        let (r, g, b) = (f32::from(px[0]), f32::from(px[1]), f32::from(px[2]));
        let luma = 0.299 * r + 0.587 * g + 0.114 * b;
        px[0] = ((r * (1.0 - d) + luma * d) * (1.0 - k)) as u8;
        px[1] = ((g * (1.0 - d) + luma * d) * (1.0 - k)) as u8;
        px[2] = ((b * (1.0 - d) + luma * d) * (1.0 - k)) as u8;
    }
}

/// Resize/pad/crop `src` per `req` and encode it.
///
/// # Errors
/// Returns an error if encoding fails or the target box is degenerate.
pub fn process(src: &DynamicImage, req: &VariantRequest) -> Result<Variant, Error> {
    let (bw, bh) = clamped_box(req, src.width(), src.height());
    if bw == 0 || bh == 0 {
        return Err(Error::ZeroDimension);
    }

    let canvas = match req.fit {
        Fit::Crop => src.resize_to_fill(bw, bh, FilterType::Lanczos3),
        Fit::Pad(fill) => {
            let (smw, smh) = small_box(bw, bh, BACKDROP_LONG);
            let mut small = src.resize_to_fill(smw, smh, FilterType::Triangle);
            if fill.blur_sigma > 0.0 {
                small = small.blur(fill.blur_sigma);
            }
            let mut small_buf = small.to_rgba8();
            if fill.darken > 0.0 || fill.desaturate > 0.0 {
                apply_tone(&mut small_buf, fill.darken, fill.desaturate);
            }
            let backdrop =
                DynamicImage::ImageRgba8(small_buf).resize_exact(bw, bh, FilterType::Triangle);

            let fg = src.resize(bw, bh, FilterType::Lanczos3); // fits within, preserves aspect
            let mut out = backdrop.to_rgba8();
            let x = i64::from((bw - fg.width()) / 2);
            let y = i64::from((bh - fg.height()) / 2);
            image::imageops::overlay(&mut out, &fg.to_rgba8(), x, y);
            DynamicImage::ImageRgba8(out)
        }
    };

    let (bytes, ext) = encode(&canvas, req.format)?;
    Ok(Variant {
        bytes,
        width: canvas.width(),
        height: canvas.height(),
        ext,
    })
}

fn encode(img: &DynamicImage, format: OutputFormat) -> Result<(Vec<u8>, &'static str), Error> {
    let mut bytes = Vec::new();
    let ext = match format {
        OutputFormat::Jpeg => {
            let rgb = img.to_rgb8();
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 90);
            encoder.encode_image(&rgb)?;
            "jpg"
        }
        OutputFormat::Png => {
            img.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)?;
            "png"
        }
    };
    Ok((bytes, ext))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
    use std::io::Cursor;

    fn jpeg_bytes(img: &DynamicImage) -> Vec<u8> {
        let mut bytes = Vec::new();
        img.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Jpeg)
            .expect("encode jpeg");
        bytes
    }

    fn gradient(w: u32, h: u32) -> DynamicImage {
        #[allow(clippy::cast_possible_truncation)]
        DynamicImage::ImageRgb8(RgbImage::from_fn(w, h, |x, y| {
            Rgb([(x % 256) as u8, (y % 256) as u8, 128])
        }))
    }

    #[test]
    fn decode_preserves_dimensions_for_unoriented_image() {
        let bytes = jpeg_bytes(&gradient(120, 80));
        let img = decode(&bytes).expect("decode");
        assert_eq!(dimensions(&img), (120, 80));
    }

    #[test]
    fn decode_applies_exif_orientation() {
        let bytes = std::fs::read("tests/fixtures/oriented.jpg").expect("fixture");
        let img = decode(&bytes).expect("decode");
        // Stored 4x2 with Orientation=6 → displayed upright 2x4.
        assert_eq!(dimensions(&img), (2, 4));
    }

    fn req(width: u32, height: Option<u32>, fit: Fit) -> VariantRequest {
        VariantRequest {
            width,
            height,
            aspect_ratio: None,
            fit,
            format: OutputFormat::Jpeg,
        }
    }

    #[test]
    fn pad_no_clamp_for_large_source() {
        let src = gradient(4000, 3000);
        // width-only → 16:9 box 1008x567; source covers it, no clamp.
        assert_eq!(
            target_dimensions(&src, &req(1008, None, Fit::Pad(PadFill::default()))),
            (1008, 567)
        );
    }

    #[test]
    fn pad_clamps_small_source() {
        let src = gradient(80, 80);
        // box 160x90; s = min(1, max(80/160, 80/90)) = 0.8889 → 142x80.
        assert_eq!(
            target_dimensions(&src, &req(160, None, Fit::Pad(PadFill::default()))),
            (142, 80)
        );
    }

    #[test]
    fn crop_no_clamp_for_large_source() {
        let src = gradient(4000, 3000);
        assert_eq!(
            target_dimensions(&src, &req(196, Some(196), Fit::Crop)),
            (196, 196)
        );
    }

    #[test]
    fn crop_clamps_small_source() {
        let src = gradient(80, 80);
        // s = min(1, min(80/200, 80/100)) = 0.4 → 80x40.
        assert_eq!(
            target_dimensions(&src, &req(200, Some(100), Fit::Crop)),
            (80, 40)
        );
    }

    fn decode_dims(bytes: &[u8]) -> (u32, u32) {
        let img = image::load_from_memory(bytes).expect("reload");
        (img.width(), img.height())
    }

    #[test]
    fn crop_outputs_exact_box_and_jpeg_magic() {
        let src = gradient(200, 100);
        let v = process(&src, &req(100, Some(100), Fit::Crop)).expect("process");
        assert_eq!((v.width, v.height), (100, 100));
        assert_eq!(v.ext, "jpg");
        assert_eq!(&v.bytes[0..3], &[0xFF, 0xD8, 0xFF]); // JPEG SOI marker
        assert_eq!(decode_dims(&v.bytes), (100, 100));
    }

    #[test]
    fn png_format_outputs_png_magic() {
        let src = gradient(200, 100);
        let mut request = req(100, Some(100), Fit::Crop);
        request.format = OutputFormat::Png;
        let v = process(&src, &request).expect("process");
        assert_eq!(v.ext, "png");
        assert_eq!(&v.bytes[0..4], &[0x89, 0x50, 0x4E, 0x47]); // PNG signature
    }

    #[test]
    fn pad_outputs_box_dimensions() {
        let src = gradient(100, 200); // portrait
        let v = process(&src, &req(160, None, Fit::Pad(PadFill::default()))).expect("process");
        // width 160 → 16:9 box 160x90; source covers it, no clamp.
        assert_eq!((v.width, v.height), (160, 90));
        assert_eq!(decode_dims(&v.bytes), (160, 90));
    }

    #[test]
    fn pad_backdrop_is_non_uniform() {
        // A vertical gradient source must yield a non-flat blurred backdrop in the
        // padded (pillarbox) region.
        #[allow(clippy::cast_possible_truncation)]
        let src = DynamicImage::ImageRgb8(RgbImage::from_fn(100, 200, |_, y| {
            Rgb([(y % 256) as u8, 0, 0])
        }));
        let v = process(&src, &req(160, None, Fit::Pad(PadFill::default()))).expect("process");
        let out = image::load_from_memory(&v.bytes).expect("reload").to_rgb8();
        // Column x=2 is inside the left pad region (fit image is ~45px wide, centered).
        let top = out.get_pixel(2, 5)[0];
        let bottom = out.get_pixel(2, 84)[0];
        assert_ne!(
            top, bottom,
            "blurred backdrop should vary with the source gradient"
        );
    }

    #[test]
    fn process_is_deterministic() {
        let src = gradient(300, 200);
        let a = process(&src, &req(256, None, Fit::Pad(PadFill::default()))).expect("a");
        let b = process(&src, &req(256, None, Fit::Pad(PadFill::default()))).expect("b");
        assert_eq!(a.bytes, b.bytes);
    }

    #[test]
    fn decode_reads_webp() {
        let src = gradient(60, 40);
        let mut bytes = Vec::new();
        src.write_to(&mut Cursor::new(&mut bytes), ImageFormat::WebP)
            .expect("encode webp");
        let decoded = decode(&bytes).expect("decode webp");
        assert_eq!(dimensions(&decoded), (60, 40));
    }

    #[test]
    fn probe_reads_dimensions_without_orientation() {
        let bytes = jpeg_bytes(&gradient(120, 80));
        assert_eq!(probe(&bytes).expect("probe"), (120, 80));
    }

    #[test]
    fn probe_applies_exif_orientation() {
        let bytes = std::fs::read("tests/fixtures/oriented.jpg").expect("fixture");
        // Stored 4x2 with Orientation=6 → displayed upright 2x4.
        assert_eq!(probe(&bytes).expect("probe"), (2, 4));
    }

    #[test]
    fn target_dimensions_from_matches_decoded_path() {
        let src = gradient(4000, 3000);
        let r = req(1008, None, Fit::Pad(PadFill::default()));
        assert_eq!(
            target_dimensions_from(src.width(), src.height(), &r),
            target_dimensions(&src, &r)
        );
    }
}
