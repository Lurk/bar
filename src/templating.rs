use crate::{
    cache::{raw_cache_path, shard_prefix},
    context::BuildContext,
    fs::seahash_checksum,
    gpx_embed::gpx,
    pages::Pages,
    render::RenderedContentCache,
    site::{DynamicPage, Feed, FeedType, Page, Site, StaticPage},
};
use cloudinary::transformation::{
    Image, Transformations,
    aspect_ratio::AspectRatio,
    background::{Auto, AutoModes, Direction, Number},
    crop_mode::CropMode,
    gravity::Gravity,
    pad_mode::PadMode,
};
use data_encoding::BASE64URL_NOPAD;
use gpxtools::{StatsArgs, calculate_stats};
use std::{
    collections::HashMap,
    hash::BuildHasher,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
};
use tera::{Function, Result, Tera, Value};
use tracing::info;
use url::Url;

const VARIANT_CACHE_VERSION: usize = 2;

const PIXEL_CACHE_CAP: usize = 4;

/// Per-build image cache split by cost: `meta` (content hash + display dims) is
/// tiny and kept for the whole build so a warm rebuild never decodes; `pixels`
/// holds the expensive decoded images, bounded to the few most-recently-used
/// sources because rendering is sequential and a source's variants are produced
/// back-to-back.
#[derive(Clone)]
pub(crate) struct ImageCache(Arc<Mutex<ImageCacheInner>>);

struct ImageCacheInner {
    meta: HashMap<PathBuf, (Arc<str>, (u32, u32))>,
    pixels: Vec<(PathBuf, Arc<imgtools::DynamicImage>)>,
}

impl ImageCache {
    pub(crate) fn new() -> Self {
        ImageCache(Arc::new(Mutex::new(ImageCacheInner {
            meta: HashMap::new(),
            pixels: Vec::new(),
        })))
    }

    fn meta(&self, source: &Path) -> Option<(Arc<str>, (u32, u32))> {
        self.0
            .lock()
            .expect("image cache poisoned")
            .meta
            .get(source)
            .cloned()
    }

    fn store_meta(&self, source: &Path, hash: Arc<str>, dims: (u32, u32)) {
        self.0
            .lock()
            .expect("image cache poisoned")
            .meta
            .insert(source.to_path_buf(), (hash, dims));
    }

    fn pixels(&self, source: &Path) -> Option<Arc<imgtools::DynamicImage>> {
        let mut inner = self.0.lock().expect("image cache poisoned");
        let pos = inner.pixels.iter().position(|(p, _)| p == source)?;
        let entry = inner.pixels.remove(pos);
        let img = entry.1.clone();
        inner.pixels.insert(0, entry);
        Some(img)
    }

    fn store_pixels(&self, source: &Path, img: Arc<imgtools::DynamicImage>) {
        let mut inner = self.0.lock().expect("image cache poisoned");
        inner.pixels.retain(|(p, _)| p != source);
        inner.pixels.insert(0, (source.to_path_buf(), img));
        inner.pixels.truncate(PIXEL_CACHE_CAP);
    }

    #[cfg(test)]
    fn pixel_len(&self) -> usize {
        self.0.lock().expect("image cache poisoned").pixels.len()
    }

    #[cfg(test)]
    fn has_pixels(&self, source: &Path) -> bool {
        self.0
            .lock()
            .expect("image cache poisoned")
            .pixels
            .iter()
            .any(|(p, _)| p == source)
    }
}

#[must_use]
pub fn get_string_arg<S: BuildHasher>(
    args: &HashMap<String, Value, S>,
    key: &str,
) -> Option<String> {
    match args.get(key) {
        Some(value) => value.as_str().map(std::string::ToString::to_string),
        None => None,
    }
}

/// # Errors
/// Returns error if the value cannot be parsed as a URL.
pub fn get_url_arg<S: BuildHasher>(
    args: &HashMap<String, Value, S>,
    key: &str,
) -> std::result::Result<Option<Url>, tera::Error> {
    match args.get(key) {
        Some(value) => match value.as_str() {
            Some(string) => Url::parse(string)
                .map(Some)
                .map_err(|e| tera::Error::msg(format!("could not parse '{string}' to url: {e}"))),
            None => Ok(None),
        },
        None => Ok(None),
    }
}

pub fn get_arc_str_arg<S: BuildHasher>(
    args: &HashMap<String, Value, S>,
    key: &str,
) -> Option<Arc<str>> {
    match args.get(key) {
        Some(value) => value.as_str().map(Arc::from),
        None => None,
    }
}

#[allow(clippy::cast_possible_truncation)]
fn get_usize_arg(args: &HashMap<String, Value>, key: &str) -> Option<usize> {
    args.get(key)
        .and_then(|value| value.as_number())
        .and_then(serde_json::Number::as_u64)
        .map(|n| n as usize)
}

/// Resolve a user-supplied, project-relative path against `project_path`,
/// rejecting any path that escapes the root via `..` or an absolute prefix.
fn resolve_in_project(project_path: &Path, raw: &str) -> Result<PathBuf> {
    let rel = raw.trim().trim_start_matches('/');
    let mut depth: usize = 0;
    for comp in Path::new(rel).components() {
        match comp {
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            Component::ParentDir => {
                depth = depth.checked_sub(1).ok_or_else(|| {
                    tera::Error::msg(format!("path '{raw}' escapes the project root"))
                })?;
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(tera::Error::msg(format!(
                    "path '{raw}' must be project-relative"
                )));
            }
        }
    }
    Ok(project_path.join(rel))
}

fn get_ar_arg(args: &HashMap<String, Value>) -> Result<Option<(u32, u32)>> {
    match (
        get_usize_arg(args, "ar_width"),
        get_usize_arg(args, "ar_height"),
    ) {
        (Some(w), Some(h)) => {
            let w = u32::try_from(w)
                .map_err(|_| tera::Error::msg(format!("ar_width {w} exceeds u32")))?;
            let h = u32::try_from(h)
                .map_err(|_| tera::Error::msg(format!("ar_height {h} exceeds u32")))?;
            Ok(Some((w, h)))
        }
        _ => Ok(None),
    }
}

fn add_page(site: Arc<Site>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let path = get_string_arg(args, "path").unwrap_or("/".to_string());
        let template = get_string_arg(args, "template").unwrap_or("index.html".to_string());
        let title = get_string_arg(args, "title").unwrap_or_default();
        let description = get_string_arg(args, "description").unwrap_or_default();
        let page_num = get_usize_arg(args, "page_num").unwrap_or(0);
        site.add_page(
            DynamicPage {
                path: path.into(),
                template: template.into(),
                title: title.into(),
                description: description.into(),
                page_num,
                content: None,
            }
            .into(),
        );
        Ok(tera::to_value(())?)
    }
}

fn add_static_file(site: Arc<Site>, project_path: Arc<PathBuf>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let path = get_arc_str_arg(args, "path")
            .ok_or_else(|| tera::Error::msg("path is required for add_static_file"))?;
        let p = &project_path;
        let source = if let Some(source) = get_arc_str_arg(args, "source") {
            resolve_in_project(p, source.as_ref())?
        } else {
            resolve_in_project(p, path.as_ref())?
        };

        site.add_page(
            StaticPage {
                destination: path.clone().trim().into(),
                source: Some(source),
                fallback: None,
            }
            .into(),
        );
        Ok(tera::to_value(path)?)
    }
}

fn add_feed(site: Arc<Site>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let path = get_arc_str_arg(args, "path")
            .ok_or_else(|| tera::Error::msg("path is required for add_feed"))?;
        let typ_str = get_arc_str_arg(args, "type")
            .ok_or_else(|| tera::Error::msg("type is required for add_feed"))?;
        let typ: FeedType =
            FeedType::try_from(typ_str).map_err(|e| tera::Error::msg(format!("{e}")))?;
        site.add_page(
            Feed {
                path: path.clone(),
                typ,
                content: None,
            }
            .into(),
        );
        Ok(tera::to_value(path)?)
    }
}

fn get_static_file(site: Arc<Site>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        if let Some(path) = get_string_arg(args, "path") {
            if let Some(page) = site.get_page(path.trim_start_matches('/')) {
                if let Page::Static(inner) = page.as_ref() {
                    let source = inner.source.as_ref().ok_or_else(|| {
                        tera::Error::msg(format!("static file '{path}' has no source"))
                    })?;
                    let hash = seahash_checksum(source)
                        .map_err(|e| tera::Error::msg(format!("failed to hash '{path}': {e}")))?;
                    return Ok(tera::to_value(format!("{path}?cb={hash}"))?);
                }
                return Err(tera::Error::msg(format!(
                    "{path} is not a path to static resource"
                )));
            }
            return Err(tera::Error::msg(format!("{path} not found")));
        }
        Err(tera::Error::msg("path is required"))
    }
}

fn get_pages_by_tag(pages: Arc<Pages>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let tag = get_string_arg(args, "tag").unwrap_or_default();
        let limit = get_usize_arg(args, "limit").unwrap_or(3);
        let offset = get_usize_arg(args, "offset").unwrap_or(0);
        let pages = pages
            .get_posts_by_tag(tag.as_str(), limit, offset)
            .ok_or_else(|| tera::Error::msg(format!("tag '{tag}' not found")))?;
        Ok(tera::to_value(pages)?)
    }
}

fn get_similar(pages: Arc<Pages>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let pid = get_string_arg(args, "pid")
            .ok_or_else(|| tera::Error::msg("pid is required for get_similar"))?;
        let limit = get_usize_arg(args, "limit").unwrap_or(3);
        Ok(tera::to_value(pages.get_similar(&pid, limit))?)
    }
}

fn get_page_by_path(
    pages: Arc<Pages>,
    rendered_cache: RenderedContentCache,
) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let path = get_string_arg(args, "path")
            .ok_or_else(|| tera::Error::msg("path is required for get_page_by_path"))?;
        let pid = path.trim_end_matches(".html");
        let page = pages.get(pid);
        let mut val = tera::to_value(page)?;
        if let Some(obj) = val.as_object_mut() {
            let cache = rendered_cache.lock().expect("rendered cache poisoned");
            if let Some(rendered) = cache.get(pid) {
                obj.insert("rendered_html".into(), rendered.html.clone().into());
                obj.insert("rendered_css".into(), rendered.css.clone().into());
            }
        }
        Ok(val)
    }
}

fn get_page_by_pid(
    pages: Arc<Pages>,
    rendered_cache: RenderedContentCache,
) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let pid = get_string_arg(args, "pid")
            .ok_or_else(|| tera::Error::msg("pid is required for get_page_by_pid"))?;
        let page = pages.get(&pid);
        let mut val = tera::to_value(page)?;
        if let Some(obj) = val.as_object_mut() {
            let cache = rendered_cache.lock().expect("rendered cache poisoned");
            if let Some(rendered) = cache.get(pid.as_str()) {
                obj.insert("rendered_html".into(), rendered.html.clone().into());
                obj.insert("rendered_css".into(), rendered.css.clone().into());
            }
        }
        Ok(val)
    }
}

#[allow(clippy::cast_possible_truncation, clippy::similar_names)]
fn get_transformations(
    with: Option<usize>,
    height: Option<usize>,
    aspect_ratio: Option<(u32, u32)>,
) -> Result<Transformations> {
    match (with, height) {
        (None, None) => Err(tera::Error::msg("width or height is required")),
        (None, Some(height)) => Ok(Transformations::Crop(CropMode::FillByHeight {
            height: height as u32,
            ar: aspect_ratio.map(|(w, h)| AspectRatio::Sides(w, h)),
            gravity: Some(Gravity::Center),
        })),
        (Some(width), None) => Ok(Transformations::Pad(PadMode::PadByWidth {
            width: width as u32,
            ar: Some(
                aspect_ratio.map_or(AspectRatio::Sides(16, 9), |(w, h)| AspectRatio::Sides(w, h)),
            ),
            gravity: Some(Gravity::Center),
            background: Some(
                Auto {
                    mode: Some(AutoModes::BorderGradient),
                    number: Some(Number::Four),
                    direction: Some(Direction::Vertical),
                    palette: None,
                }
                .into(),
            ),
        })),
        (Some(width), Some(height)) => Ok(Transformations::Crop(CropMode::Fill {
            width: width as u32,
            height: height as u32,
            gravity: Some(Gravity::Center),
        })),
    }
}

pub(crate) fn gpx_srcset_string(pairs: &[(String, u32)]) -> String {
    pairs
        .iter()
        .map(|(url, w)| format!("{url} {w}w"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn srcset_for(
    site: &Site,
    project_path: &Path,
    cache: &ImageCache,
    image_output_dir: &str,
    src: &str,
    ar: Option<(u32, u32)>,
    widths: &[usize],
) -> std::result::Result<String, tera::Error> {
    let mut entries: Vec<(String, u32)> = Vec::new();
    let mut seen: Vec<u32> = Vec::new();
    for &width in widths {
        let (url, ow) = image_variant(
            site,
            project_path,
            cache,
            image_output_dir,
            VariantSpec {
                src,
                width,
                height: None,
                ar,
            },
        )?;
        match ow {
            Some(w) => {
                if !seen.contains(&w) {
                    seen.push(w);
                    entries.push((url, w));
                }
            }
            // Passthrough source (e.g. svg): a single candidate, no descriptor.
            None => return Ok(url),
        }
    }
    Ok(gpx_srcset_string(&entries))
}

/// Content hash, display dims, and the file bytes on a cache miss.
type SourceMeta = (Arc<str>, (u32, u32), Option<Vec<u8>>);

/// Content hash + display dims for `source`, memoized in `cache`. On a cache
/// miss the file is read and its header probed (no pixel decode); the read bytes
/// are returned so a following decode can reuse them instead of re-reading.
fn source_metadata(
    cache: &ImageCache,
    source: &Path,
) -> std::result::Result<SourceMeta, tera::Error> {
    if let Some((hash, dims)) = cache.meta(source) {
        return Ok((hash, dims, None));
    }
    let bytes = std::fs::read(source).map_err(|e| {
        tera::Error::msg(format!("failed to read image '{}': {e}", source.display()))
    })?;
    let hash: Arc<str> =
        Arc::from(BASE64URL_NOPAD.encode(seahash::hash(&bytes).to_be_bytes().as_ref()));
    let dims = imgtools::probe(&bytes).map_err(|e| {
        tera::Error::msg(format!(
            "failed to read image header '{}': {e}",
            source.display()
        ))
    })?;
    cache.store_meta(source, hash.clone(), dims);
    Ok((hash, dims, Some(bytes)))
}

/// Decoded `source`, served from the pixel LRU when present. On a miss it decodes
/// (reusing `bytes` if the caller already read them, else reading) and inserts
/// the result into the LRU.
fn decoded_source(
    cache: &ImageCache,
    source: &Path,
    bytes: Option<Vec<u8>>,
) -> std::result::Result<Arc<imgtools::DynamicImage>, tera::Error> {
    if let Some(img) = cache.pixels(source) {
        return Ok(img);
    }
    let raw = if let Some(bytes) = bytes {
        bytes
    } else {
        std::fs::read(source).map_err(|e| {
            tera::Error::msg(format!("failed to read image '{}': {e}", source.display()))
        })?
    };
    let decoded = Arc::new(imgtools::decode(&raw).map_err(|e| {
        tera::Error::msg(format!(
            "failed to decode image '{}': {e}",
            source.display()
        ))
    })?);
    cache.store_pixels(source, decoded.clone());
    Ok(decoded)
}

#[derive(Clone, Copy)]
pub(crate) struct VariantSpec<'a> {
    pub src: &'a str,
    pub width: usize,
    pub height: Option<usize>,
    pub ar: Option<(u32, u32)>,
}

pub(crate) fn image_variant(
    site: &Site,
    project_path: &Path,
    cache: &ImageCache,
    image_output_dir: &str,
    spec: VariantSpec<'_>,
) -> std::result::Result<(String, Option<u32>), tera::Error> {
    let VariantSpec {
        src,
        width,
        height,
        ar,
    } = spec;
    if src.is_empty() {
        return Ok((src.to_string(), None));
    }

    let width_u32 =
        u32::try_from(width).map_err(|_| tera::Error::msg(format!("width {width} exceeds u32")))?;
    let height_u32 = height
        .map(u32::try_from)
        .transpose()
        .map_err(|_| tera::Error::msg(format!("height {height:?} exceeds u32")))?;

    if src.starts_with('/') {
        let source = resolve_in_project(project_path, src)?;
        let ext = source
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        let format = match ext.as_deref() {
            Some("png") => imgtools::OutputFormat::Png,
            Some("jpg" | "jpeg" | "webp") => imgtools::OutputFormat::Jpeg,
            _ => {
                site.add_page(
                    StaticPage {
                        destination: src.trim().into(),
                        source: Some(source),
                        fallback: None,
                    }
                    .into(),
                );
                return Ok((src.to_string(), None));
            }
        };

        // Cheap metadata: content hash + display dims, memoized for the whole
        // build so a warm rebuild never reaches a pixel decode. `bytes` carries
        // the just-read file forward so a following decode need not re-read it.
        let (hash, dims, bytes) = source_metadata(cache, &source)?;

        let fit = if height.is_some() {
            imgtools::Fit::Crop
        } else {
            imgtools::Fit::Pad(imgtools::PadFill::default())
        };
        let request = imgtools::VariantRequest {
            width: width_u32,
            height: height_u32,
            aspect_ratio: ar,
            fit,
            format,
        };
        let (ow, oh) = imgtools::target_dimensions_from(dims.0, dims.1, &request);
        let ext_str = match format {
            imgtools::OutputFormat::Png => "png",
            imgtools::OutputFormat::Jpeg => "jpg",
        };
        // The fit tag keeps pad and crop variants apart: both can resolve to the
        // same output dimensions from the same source yet hold different pixels.
        let fit_tag = match fit {
            imgtools::Fit::Pad(_) => 'p',
            imgtools::Fit::Crop => 'c',
        };
        // The version lives in the filename rather than in `Cache`'s versioning,
        // so bumping `VARIANT_CACHE_VERSION` simply changes the key and orphans
        // stale variants.
        let key = format!("v{VARIANT_CACHE_VERSION}-{hash}-{fit_tag}-{ow}x{oh}");
        // Shard the on-disk path by the source hash (as `Cache::make_key` does)
        // so no single directory holds every gallery's variants. The published
        // dist path stays flat.
        let cache_key = format!("{}/{key}", shard_prefix(&hash));
        let cache_path = raw_cache_path(project_path, "image_variants", &cache_key, ext_str);
        if !cache_path.exists() {
            // Cache miss: decode the source now. The decoded image is held in a
            // small LRU so a source's other ladder/thumbnail misses reuse it.
            let img = decoded_source(cache, &source, bytes)?;
            let variant = imgtools::process(&img, &request)
                .map_err(|e| tera::Error::msg(format!("failed to process image '{src}': {e}")))?;
            if let Some(parent) = cache_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    tera::Error::msg(format!(
                        "failed to create variant dir '{}': {e}",
                        parent.display()
                    ))
                })?;
            }
            std::fs::write(&cache_path, &variant.bytes).map_err(|e| {
                tera::Error::msg(format!(
                    "failed to write variant '{}': {e}",
                    cache_path.display()
                ))
            })?;
        }
        let rel_dest = format!("{}/{}.{}", image_output_dir.trim_matches('/'), key, ext_str);
        let destination = format!("/{rel_dest}");
        site.add_page(
            StaticPage {
                destination: rel_dest.into(),
                source: Some(cache_path),
                fallback: None,
            }
            .into(),
        );
        return Ok((destination, Some(ow)));
    }

    remote_variant(src, width, width_u32, height, ar)
}

fn remote_variant(
    src: &str,
    width: usize,
    width_u32: u32,
    height: Option<usize>,
    ar: Option<(u32, u32)>,
) -> std::result::Result<(String, Option<u32>), tera::Error> {
    let url = Url::parse(src)
        .map_err(|e| tera::Error::msg(format!("failed to parse url '{src}': {e}")))?;
    match Image::try_from(url.clone()) {
        Ok(image) => {
            let transformation = get_transformations(Some(width), height, ar)?;
            let result = image.add_transformation(transformation);
            Ok((result.to_string(), Some(width_u32)))
        }
        // A non-Cloudinary remote URL can't be resized, so it's a single
        // passthrough candidate (no descriptor) rather than one fabricated
        // srcset width per ladder step.
        Err(_) => Ok((url.to_string(), None)),
    }
}

#[allow(clippy::cast_possible_truncation)]
fn get_image_url(
    site: Arc<Site>,
    config: Arc<crate::config::Config>,
    project_path: Arc<PathBuf>,
    cache: ImageCache,
) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let src = get_string_arg(args, "src")
            .ok_or_else(|| tera::Error::msg("src is required for get_image_url"))?;
        let height = get_usize_arg(args, "height");
        let ar = get_ar_arg(args)?;
        let Some(width) = get_usize_arg(args, "width").or(height) else {
            // No dimensions: preserve the historical passthrough for local paths.
            if src.starts_with('/') {
                site.add_page(
                    StaticPage {
                        destination: src.trim().into(),
                        source: Some(resolve_in_project(&project_path, &src)?),
                        fallback: None,
                    }
                    .into(),
                );
                return Ok(tera::to_value(src)?);
            }
            return Err(tera::Error::msg(
                "width or height is required for get_image_url",
            ));
        };
        // `width` is the width arg when present, else the height (square-ish crop).
        let (url, _) = image_variant(
            &site,
            project_path.as_path(),
            &cache,
            &config.image_output_dir,
            VariantSpec {
                src: &src,
                width,
                height,
                ar,
            },
        )?;
        Ok(tera::to_value(url)?)
    }
}

fn get_srcset(
    site: Arc<Site>,
    config: Arc<crate::config::Config>,
    project_path: Arc<PathBuf>,
    cache: ImageCache,
    widths: Arc<Vec<usize>>,
) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let src = get_string_arg(args, "src")
            .ok_or_else(|| tera::Error::msg("src is required for get_srcset"))?;
        let ar = get_ar_arg(args)?;
        let srcset = srcset_for(
            &site,
            project_path.as_path(),
            &cache,
            &config.image_output_dir,
            &src,
            ar,
            &widths,
        )?;
        Ok(tera::to_value(srcset)?)
    }
}

#[allow(clippy::cast_precision_loss)]
fn get_gpx_srcset(
    site: Arc<Site>,
    config: Arc<crate::config::Config>,
    project_path: Arc<PathBuf>,
    widths: Arc<Vec<usize>>,
) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let input = get_string_arg(args, "input")
            .ok_or_else(|| tera::Error::msg("input is required for get_gpx_srcset"))?;
        let base = config.gpx_embedding.base.clone();
        let copyright = config.gpx_embedding.attribution_png.clone();
        let handle = tokio::runtime::Handle::try_current().map_err(|e| format!("{e}"))?;

        let gpx_input = resolve_in_project(&project_path, &input)?;
        let mut pairs: Vec<(String, u32)> = Vec::new();
        for &w in widths.iter() {
            let w_u32 =
                u32::try_from(w).map_err(|_| tera::Error::msg(format!("width {w} exceeds u32")))?;
            let height = (w as f64 * 9.0 / 16.0).round();
            let url = handle
                .block_on(async {
                    gpx(
                        site.clone(),
                        base.clone(),
                        copyright.clone(),
                        gpx_input.clone(),
                        w as f64,
                        height,
                        project_path.as_ref().clone(),
                    )
                    .await
                })
                .map_err(|e| format!("{e}"))?;
            pairs.push((url, w_u32));
        }
        Ok(tera::to_value(gpx_srcset_string(&pairs))?)
    }
}

#[allow(clippy::cast_precision_loss)]
fn render_gpx(
    site: Arc<Site>,
    config: Arc<crate::config::Config>,
    project_path: Arc<PathBuf>,
) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let input = get_string_arg(args, "input")
            .ok_or_else(|| tera::Error::msg("input is required for render_gpx"))?;
        let width = get_usize_arg(args, "width").unwrap_or(800) as f64;
        let height = get_usize_arg(args, "height").unwrap_or(600) as f64;
        let base = config.gpx_embedding.base.clone();
        let copyright = config.gpx_embedding.attribution_png.clone();
        let gpx_input = resolve_in_project(&project_path, &input)?;

        let map_url = tokio::runtime::Handle::try_current()
            .map_err(|e| format!("{e}"))?
            .block_on(async {
                gpx(
                    site.clone(),
                    base,
                    copyright,
                    gpx_input,
                    width,
                    height,
                    project_path.as_ref().clone(),
                )
                .await
            })
            .map_err(|e| format!("{e}"))?;
        Ok(tera::to_value(map_url)?)
    }
}

fn get_gpx_stats(project_path: Arc<PathBuf>) -> impl Function + 'static {
    move |args: &HashMap<String, Value>| {
        let input_arg = get_string_arg(args, "input")
            .ok_or_else(|| tera::Error::msg("input is required for get_gpx_stats"))?;
        let input = resolve_in_project(&project_path, &input_arg)?;

        let stats = calculate_stats(&StatsArgs {
            input: vec![input.clone()],
        })
        .map_err(|e| format!("{e}"))?;

        let stat = stats
            .first()
            .ok_or_else(|| tera::Error::msg(format!("no stats for {}", input.display())))?;

        Ok(tera::to_value(stat)?)
    }
}

fn crc32(value: &Value, _: &HashMap<String, Value>) -> Result<Value> {
    let val = value
        .as_str()
        .ok_or_else(|| tera::Error::msg("crc32 filter requires a string value"))?;
    Ok(tera::to_value(BASE64URL_NOPAD.encode(
        seahash::hash(val.as_bytes()).to_be_bytes().as_ref(),
    ))?)
}

pub fn register_functions(
    tera: &mut Tera,
    site: Arc<Site>,
    config: Arc<crate::config::Config>,
    project_path: Arc<PathBuf>,
    pages: &Arc<Pages>,
    rendered_cache: RenderedContentCache,
    image_widths: Arc<Vec<usize>>,
) {
    let image_cache = ImageCache::new();
    tera.register_function("add_feed", add_feed(site.clone()));
    tera.register_function("add_page", add_page(site.clone()));
    tera.register_function(
        "add_static_file",
        add_static_file(site.clone(), project_path.clone()),
    );
    tera.register_function("get_gpx_stats", get_gpx_stats(project_path.clone()));
    tera.register_function(
        "get_image_url",
        get_image_url(
            site.clone(),
            config.clone(),
            project_path.clone(),
            image_cache.clone(),
        ),
    );
    tera.register_function(
        "get_srcset",
        get_srcset(
            site.clone(),
            config.clone(),
            project_path.clone(),
            image_cache,
            image_widths.clone(),
        ),
    );
    tera.register_function(
        "get_gpx_srcset",
        get_gpx_srcset(
            site.clone(),
            config.clone(),
            project_path.clone(),
            image_widths,
        ),
    );
    tera.register_function("get_pages_by_tag", get_pages_by_tag(pages.clone()));
    tera.register_function(
        "get_page_by_path",
        get_page_by_path(pages.clone(), rendered_cache.clone()),
    );
    tera.register_function(
        "get_page_by_pid",
        get_page_by_pid(pages.clone(), rendered_cache),
    );
    tera.register_function("get_similar", get_similar(pages.clone()));
    tera.register_function("get_static_file", get_static_file(site.clone()));
    tera.register_function("render_gpx", render_gpx(site, config, project_path));
    tera.register_filter("crc32", crc32);
}

/// # Errors
/// Returns error if templates cannot be loaded or parsed.
pub fn initialize(
    ctx: &BuildContext,
    template_path: &Path,
    rendered_cache: RenderedContentCache,
) -> Result<Tera> {
    let templates = format!(
        "{}/**/*.html",
        template_path
            .to_str()
            .ok_or_else(|| tera::Error::msg(format!(
                "template path is not valid UTF-8: {}",
                template_path.display()
            )))?
    );
    info!("initialize teplates: {}", templates);
    let project_path = Arc::new(ctx.config.path.clone());
    let config = Arc::new(ctx.config.config.clone());
    let image_widths = Arc::new(ctx.theme.render.image.widths());
    let mut tera = Tera::new(&templates)?;
    register_functions(
        &mut tera,
        ctx.site.clone(),
        config,
        project_path,
        &ctx.pages,
        rendered_cache,
        image_widths,
    );
    info!("template initialization complete");
    Ok(tera)
}

#[cfg(test)]
mod tests {
    use super::{
        ImageCache, VariantSpec, get_transformations, image_variant, resolve_in_project, srcset_for,
    };
    use crate::site::{Page, Site};
    use std::path::Path;
    use std::sync::Arc;

    #[test]
    fn resolve_in_project_keeps_relative_paths_inside_root() {
        let root = Path::new("/proj");
        assert_eq!(
            resolve_in_project(root, "/images/a.jpg").unwrap(),
            root.join("images/a.jpg")
        );
        assert_eq!(
            resolve_in_project(root, "tracks/../tracks/run.gpx").unwrap(),
            root.join("tracks/../tracks/run.gpx")
        );
    }

    #[test]
    fn resolve_in_project_rejects_escaping_paths() {
        let root = Path::new("/proj");
        assert!(resolve_in_project(root, "/../../etc/passwd").is_err());
        assert!(resolve_in_project(root, "a/../../b").is_err());
    }

    #[allow(clippy::cast_possible_truncation)]
    fn write_image(path: &Path, w: u32, h: u32, format: image::ImageFormat) {
        use image::{DynamicImage, Rgb, RgbImage};
        let img = DynamicImage::ImageRgb8(RgbImage::from_fn(w, h, |x, y| {
            Rgb([(x % 256) as u8, (y % 256) as u8, 128])
        }));
        img.save_with_format(path, format).expect("write image");
    }

    fn dummy_img() -> Arc<imgtools::DynamicImage> {
        Arc::new(imgtools::DynamicImage::new_rgb8(1, 1))
    }

    #[test]
    fn image_cache_meta_roundtrip() {
        let cache = ImageCache::new();
        assert!(cache.meta(Path::new("/x")).is_none());
        cache.store_meta(Path::new("/x"), Arc::from("hsh"), (12, 34));
        let (h, d) = cache.meta(Path::new("/x")).expect("meta");
        assert_eq!((&*h, d), ("hsh", (12, 34)));
    }

    #[test]
    fn image_cache_pixels_evict_oldest_beyond_cap() {
        let cache = ImageCache::new();
        for i in 0..5 {
            cache.store_pixels(Path::new(&format!("/p{i}")), dummy_img());
        }
        assert_eq!(cache.pixel_len(), 4);
        assert!(!cache.has_pixels(Path::new("/p0")), "oldest evicted");
        assert!(cache.has_pixels(Path::new("/p4")), "newest retained");
    }

    #[test]
    fn image_cache_pixels_move_to_front_on_access() {
        let cache = ImageCache::new();
        for i in 0..4 {
            cache.store_pixels(Path::new(&format!("/p{i}")), dummy_img());
        }
        // Touch p0 so p1 becomes the least-recently-used entry.
        assert!(cache.pixels(Path::new("/p0")).is_some());
        cache.store_pixels(Path::new("/p4"), dummy_img());
        assert!(cache.has_pixels(Path::new("/p0")), "touched entry survived");
        assert!(!cache.has_pixels(Path::new("/p1")), "p1 was LRU, evicted");
    }

    /// Locate a variant file by basename anywhere under the cache root, so tests
    /// stay agnostic to the on-disk sharding.
    fn find_variant(root: &Path, fname: &str) -> Option<std::path::PathBuf> {
        for entry in std::fs::read_dir(root).ok()?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(found) = find_variant(&path, fname) {
                    return Some(found);
                }
            } else if path.file_name().and_then(|n| n.to_str()) == Some(fname) {
                return Some(path);
            }
        }
        None
    }

    #[test]
    fn get_transformations_test() {
        assert!(get_transformations(None, None, None).is_err());
        assert!(get_transformations(Some(100), None, None).is_ok());
        assert!(get_transformations(None, Some(100), None).is_ok());
        assert!(get_transformations(Some(100), Some(100), None).is_ok());
        assert!(get_transformations(Some(100), None, Some((4, 3))).is_ok());
        assert!(get_transformations(None, Some(100), Some((4, 3))).is_ok());
    }

    #[test]
    fn local_webp_produces_jpeg_variant() {
        let dir = tempfile::tempdir().unwrap();
        write_image(
            &dir.path().join("photo.webp"),
            400,
            300,
            image::ImageFormat::WebP,
        );
        let site = Arc::new(Site::new(dir.path().join("dist")));

        let (url, width) = image_variant(
            &site,
            dir.path(),
            &ImageCache::new(),
            "images",
            VariantSpec {
                src: "/photo.webp",
                width: 1008,
                height: None,
                ar: None,
            },
        )
        .expect("variant");

        // width-only → 16:9 box 1008x567; clamp to source 400x300 → 533x300.
        assert_eq!(width, Some(533));
        assert!(
            url.starts_with("/images/") && url.ends_with("-533x300.jpg"),
            "got: {url}"
        );
        let fname = url.rsplit('/').next().unwrap();
        assert!(find_variant(&dir.path().join(".cache/image_variants"), fname).is_some());
        assert!(matches!(
            site.get_page(url.trim_start_matches('/')).as_deref(),
            Some(Page::Static(_))
        ));
    }

    #[test]
    fn local_jpeg_produces_padded_variant_and_static_page() {
        let dir = tempfile::tempdir().unwrap();
        write_image(
            &dir.path().join("photo.jpg"),
            400,
            300,
            image::ImageFormat::Jpeg,
        );
        let site = Arc::new(Site::new(dir.path().join("dist")));
        let cache = ImageCache::new();

        let (url, width) = image_variant(
            &site,
            dir.path(),
            &cache,
            "images",
            VariantSpec {
                src: "/photo.jpg",
                width: 1008,
                height: None,
                ar: None,
            },
        )
        .expect("variant");

        // width-only → 16:9 box 1008x567; clamp to source 400x300 → 533x300.
        assert_eq!(width, Some(533));
        assert!(
            url.starts_with("/images/") && url.ends_with("-533x300.jpg"),
            "got: {url}"
        );
        let fname = url.rsplit('/').next().unwrap();
        assert!(find_variant(&dir.path().join(".cache/image_variants"), fname).is_some());
        assert!(matches!(
            site.get_page(url.trim_start_matches('/')).as_deref(),
            Some(Page::Static(_))
        ));
    }

    #[test]
    fn local_png_keeps_png_extension() {
        let dir = tempfile::tempdir().unwrap();
        write_image(
            &dir.path().join("logo.png"),
            300,
            300,
            image::ImageFormat::Png,
        );
        let site = Arc::new(Site::new(dir.path().join("dist")));
        let (url, _) = image_variant(
            &site,
            dir.path(),
            &ImageCache::new(),
            "images",
            VariantSpec {
                src: "/logo.png",
                width: 200,
                height: Some(200),
                ar: None,
            },
        )
        .expect("variant");
        #[allow(clippy::case_sensitive_file_extension_comparisons)]
        let is_png = url.ends_with(".png");
        assert!(is_png, "got: {url}");
    }

    #[test]
    fn cache_hit_does_not_reprocess() {
        let dir = tempfile::tempdir().unwrap();
        write_image(
            &dir.path().join("photo.jpg"),
            400,
            300,
            image::ImageFormat::Jpeg,
        );
        let site = Arc::new(Site::new(dir.path().join("dist")));
        let cache = ImageCache::new();

        let (url, _) = image_variant(
            &site,
            dir.path(),
            &cache,
            "images",
            VariantSpec {
                src: "/photo.jpg",
                width: 1008,
                height: None,
                ar: None,
            },
        )
        .unwrap();
        let fname = url.rsplit('/').next().unwrap();
        let cache_path =
            find_variant(&dir.path().join(".cache/image_variants"), fname).expect("variant file");
        std::fs::write(&cache_path, b"SENTINEL").unwrap();

        let _ = image_variant(
            &site,
            dir.path(),
            &cache,
            "images",
            VariantSpec {
                src: "/photo.jpg",
                width: 1008,
                height: None,
                ar: None,
            },
        )
        .unwrap();
        assert_eq!(std::fs::read(&cache_path).unwrap(), b"SENTINEL");
    }

    #[test]
    fn non_raster_local_passes_through() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("icon.svg"), "<svg/>").unwrap();
        let site = Arc::new(Site::new(dir.path().join("dist")));
        let (url, width) = image_variant(
            &site,
            dir.path(),
            &ImageCache::new(),
            "images",
            VariantSpec {
                src: "/icon.svg",
                width: 1008,
                height: None,
                ar: None,
            },
        )
        .expect("variant");
        assert_eq!(url, "/icon.svg");
        assert_eq!(width, None);
    }

    #[test]
    fn srcset_descriptors_match_files_and_dedup() {
        let dir = tempfile::tempdir().unwrap();
        write_image(
            &dir.path().join("photo.jpg"),
            400,
            300,
            image::ImageFormat::Jpeg,
        );
        let site = Arc::new(Site::new(dir.path().join("dist")));

        let srcset = srcset_for(
            &site,
            dir.path(),
            &ImageCache::new(),
            "images",
            "/photo.jpg",
            None,
            &[352, 704, 1008, 1568, 2016, 3840],
        )
        .expect("srcset");

        // Source 400x300 clamps every pad box wider than the source to 533x300,
        // so only 352w (unclamped) and 533w (everything larger, deduped) survive.
        let entries: Vec<&str> = srcset.split(", ").collect();
        assert_eq!(entries.len(), 2, "got: {srcset}");
        assert!(srcset.contains("-352x198.jpg 352w"), "got: {srcset}");
        assert!(srcset.contains("-533x300.jpg 533w"), "got: {srcset}");
        for entry in entries {
            let (url, desc) = entry.rsplit_once(' ').unwrap();
            let w: &str = desc.trim_end_matches('w');
            assert!(url.contains(&format!("-{w}x")), "descriptor lies: {entry}");
        }
    }

    #[test]
    fn srcset_for_cloudinary_emits_ladder_widths() {
        let dir = tempfile::tempdir().unwrap();
        let site = Arc::new(Site::new(dir.path().join("dist")));
        let srcset = srcset_for(
            &site,
            dir.path(),
            &ImageCache::new(),
            "images",
            "https://res.cloudinary.com/demo/image/upload/sample.jpg",
            None,
            &[352, 704, 1008, 1568, 2016, 3840],
        )
        .expect("srcset");
        assert_eq!(srcset.split(", ").count(), 6, "got: {srcset}");
        assert!(srcset.contains("w_3840"), "got: {srcset}");
        assert!(srcset.contains(" 3840w"), "got: {srcset}");
    }

    #[test]
    fn srcset_for_non_cloudinary_url_is_single_passthrough() {
        let dir = tempfile::tempdir().unwrap();
        let site = Arc::new(Site::new(dir.path().join("dist")));
        let srcset = srcset_for(
            &site,
            dir.path(),
            &ImageCache::new(),
            "images",
            "https://example.com/p.jpg",
            None,
            &[352, 704, 1008, 1568, 2016, 3840],
        )
        .expect("srcset");
        // A non-Cloudinary remote URL cannot be resized, so it is a single
        // passthrough candidate with no descriptor rather than the same URL
        // repeated across the ladder with fabricated widths.
        assert_eq!(srcset, "https://example.com/p.jpg", "got: {srcset}");
    }

    #[test]
    fn pad_and_crop_to_same_dimensions_do_not_collide() {
        let dir = tempfile::tempdir().unwrap();
        // Square source: pad into the default 16:9 box and an explicit crop both
        // resolve to 352x198, but the pixels (blurred bars vs. center crop) differ.
        write_image(
            &dir.path().join("square.jpg"),
            1000,
            1000,
            image::ImageFormat::Jpeg,
        );
        let site = Arc::new(Site::new(dir.path().join("dist")));
        let cache = ImageCache::new();

        let (pad_url, _) = image_variant(
            &site,
            dir.path(),
            &cache,
            "images",
            VariantSpec {
                src: "/square.jpg",
                width: 352,
                height: None,
                ar: None,
            },
        )
        .expect("pad");
        let (crop_url, _) = image_variant(
            &site,
            dir.path(),
            &cache,
            "images",
            VariantSpec {
                src: "/square.jpg",
                width: 352,
                height: Some(198),
                ar: None,
            },
        )
        .expect("crop");

        assert_ne!(
            pad_url, crop_url,
            "pad and crop variants at the same dimensions must not share a cache file"
        );
        let read = |url: &str| {
            let fname = url.rsplit('/').next().unwrap();
            let path = find_variant(&dir.path().join(".cache/image_variants"), fname)
                .expect("variant file");
            std::fs::read(path).unwrap()
        };
        assert_ne!(read(&pad_url), read(&crop_url), "variant bytes must differ");
    }

    #[test]
    fn variant_is_sharded_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        write_image(
            &dir.path().join("photo.jpg"),
            400,
            300,
            image::ImageFormat::Jpeg,
        );
        let site = Arc::new(Site::new(dir.path().join("dist")));
        let (url, _) = image_variant(
            &site,
            dir.path(),
            &ImageCache::new(),
            "images",
            VariantSpec {
                src: "/photo.jpg",
                width: 1008,
                height: None,
                ar: None,
            },
        )
        .expect("variant");

        // Variant files live under two shard dirs (mirroring `Cache::make_key`),
        // so no single directory accumulates every gallery's images.
        let fname = url.rsplit('/').next().unwrap();
        let path = find_variant(&dir.path().join(".cache/image_variants"), fname).expect("file");
        let shard = path
            .strip_prefix(dir.path().join(".cache/image_variants"))
            .unwrap();
        assert_eq!(
            shard.components().count(),
            3,
            "expected <shard>/<shard>/<file>, got: {}",
            shard.display()
        );
    }

    #[test]
    fn hit_path_does_not_touch_source() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("photo.jpg");
        write_image(&source, 400, 300, image::ImageFormat::Jpeg);
        let site = Arc::new(Site::new(dir.path().join("dist")));
        let cache = ImageCache::new();

        // Warm: produce the variant and populate meta + on-disk cache.
        let (url, width) = image_variant(
            &site,
            dir.path(),
            &cache,
            "images",
            VariantSpec {
                src: "/photo.jpg",
                width: 1008,
                height: None,
                ar: None,
            },
        )
        .expect("warm");
        assert_eq!(width, Some(533));

        // Delete the source. A pure hit (meta hit + output on disk) must not read
        // or decode it.
        std::fs::remove_file(&source).unwrap();

        let site2 = Arc::new(Site::new(dir.path().join("dist")));
        let (url2, width2) = image_variant(
            &site2,
            dir.path(),
            &cache,
            "images",
            VariantSpec {
                src: "/photo.jpg",
                width: 1008,
                height: None,
                ar: None,
            },
        )
        .expect("hit must succeed without the source file");
        assert_eq!((url2, width2), (url, Some(533)));
    }

    #[test]
    fn gpx_srcset_descriptors_use_configured_widths() {
        // gpx_srcset_string maps (url, width) pairs into a "url Nw, ..." srcset.
        let pairs = vec![
            ("/m-352x198.png".to_string(), 352u32),
            ("/m-1008x567.png".to_string(), 1008u32),
        ];
        let srcset = super::gpx_srcset_string(&pairs);
        assert_eq!(srcset, "/m-352x198.png 352w, /m-1008x567.png 1008w");
    }

    #[test]
    fn pixel_cache_bounds_across_variant_calls() {
        let dir = tempfile::tempdir().unwrap();
        let site = Arc::new(Site::new(dir.path().join("dist")));
        let cache = ImageCache::new();
        for i in 0u32..5 {
            let name = format!("img{i}.jpg");
            // Distinct dimensions per source so each has a unique hash and on-disk
            // variant path — otherwise identical files collapse to one disk entry
            // and only the first source ever decodes.
            write_image(
                &dir.path().join(&name),
                400 + i,
                300,
                image::ImageFormat::Jpeg,
            );
            image_variant(
                &site,
                dir.path(),
                &cache,
                "images",
                VariantSpec {
                    src: &format!("/{name}"),
                    width: 1008,
                    height: None,
                    ar: None,
                },
            )
            .expect("variant");
        }
        assert_eq!(cache.pixel_len(), 4);
        assert!(
            !cache.has_pixels(&dir.path().join("img0.jpg")),
            "oldest evicted"
        );
        assert!(
            cache.has_pixels(&dir.path().join("img4.jpg")),
            "newest retained"
        );
    }
}
