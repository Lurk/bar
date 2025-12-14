use std::{collections::HashMap, f64::consts::PI, path::PathBuf, pin::Pin, sync::Arc};

use geo::{Point, Rect, Scale};
use tiny_skia::{
    Color, LineCap, LineJoin, Paint, PathBuilder, Pixmap, PixmapPaint, Shader, Stroke, StrokeDash,
    Transform,
};
use tokio::{fs::create_dir_all, task::JoinSet};

use crate::{
    GPXError, error,
    utils::{Line, read_gpx_file},
};

#[derive(clap::Args)]
pub struct PlotArgs {
    /// Input file path
    #[clap(short, long)]
    pub input: PathBuf,
    /// Output image file path
    #[clap(short, long)]
    pub output: PathBuf,
    ///width of the output image in pixels
    #[clap(long, short, default_value_t = 4096.)]
    pub width: f64,
    ///height of the output image in pixels
    #[clap(long, short, default_value_t = 2_304.)]
    pub height: f64,
    /// Base URL for tile server (should support {z}/{x}/{y} pattern)
    #[clap(long, short, default_value = "https://tile.openstreetmap.org")]
    pub base: Vec<Arc<str>>,
    /// Tiles copyright notice file path. PNG, will be embedded into right bottom corner.
    #[clap(long)]
    pub attribution_png: Option<PathBuf>,
    /// Overwrite output file if it exists
    #[clap(long, short, default_value_t = false)]
    pub force: bool,
}

/// Plots GPX tracks onto an image using map tiles and saves the result.
///
/// # Arguments
/// * `getter` - An async function or closure that fetches map tiles given a URL.
/// * `plot` - Arguments configuring the plot operation (input/output paths, image size, tile server, etc.).
///
/// # Returns
/// * `Ok(())` if the plot was generated and saved successfully.
/// * `Err(GPXError)` if an error occurred during processing.
///
/// # Example
/// ```rust
/// use gpxtools::plot::{plot, PlotArgs};
/// use std::sync::Arc;
/// use std::path::PathBuf;
///
/// async fn tile_getter(url: String) -> Result<(String, Vec<u8>), String> {
///     // Fetch tile data from the URL
///     // Return Ok((url, bytes)) or Err(error_message)
///     unimplemented!()
/// }
///
/// let args = PlotArgs {
///     input: PathBuf::from("track.gpx"),
///     output: PathBuf::from("output.png"),
///     width: 1024.0,
///     height: 768.0,
///     base: vec![Arc::from("https://tile.openstreetmap.org")],
///     attribution_png: None,
///     force: true,
/// };
///
/// async {
///     let result = plot(|src| Box::pin(async move { tile_getter(src).await }), args).await;
///     match result {
///         Ok(()) => println!("Plot generated!"),
///         Err(e) => eprintln!("Error: {:?}", e),
///     }
/// };
/// ```
pub async fn plot<F, Fut>(getter: F, plot: PlotArgs) -> Result<(), error::GPXError>
where
    Fut: Future<Output = Result<(String, Vec<u8>), String>> + 'static + Send,
    F: Fn(String) -> Pin<Box<Fut>>,
{
    let gpx = read_gpx_file(&plot.input)?;
    let br = Line::new(&gpx.tracks)
        .get_bounding_rect()
        .expect("bounding rect exists")
        .scale(1.1);

    let mut map = Map::new(View::from((br, plot.width, plot.height)));

    map.fill_tiles(&getter, plot.base).await?;
    map.plot_path(Line::new(&gpx.tracks).map(|wpt| wpt.point()));

    if let Some(attribution_path) = plot.attribution_png {
        map.add_attribution(attribution_path);
    }

    create_dir_all(plot.output.parent().unwrap())
        .await
        .expect("Output directory should be created");

    map.pixmap
        .save_png(plot.output)
        .expect("Pixmap should be saved");

    Ok(())
}

/// Represents the visible area of the map, including the zoom level and bounding rectangle.
/// Used to calculate which tiles to load and how to transform coordinates for rendering.
struct View {
    zoom: Zoom,
    bounding_rect: Rect,
}

impl View {
    /// Creates a new `View` with the specified zoom level and bounding rectangle.
    fn new(zoom: Zoom, bounding_rect: Rect) -> Self {
        Self {
            zoom,
            bounding_rect,
        }
    }

    /// Returns the width and height (in pixels) of the view at the current zoom level.
    fn get_dimensions(&self) -> (f64, f64) {
        let max = self
            .zoom
            .lonlat2tile(self.bounding_rect.max().x, self.bounding_rect.min().y);
        let min = self
            .zoom
            .lonlat2tile(self.bounding_rect.min().x, self.bounding_rect.max().y);

        let width = (max.0 - min.0) * 256.;
        let height = (max.1 - min.1) * 256.;

        (width, height)
    }

    /// Determines if the view can fit within the specified width and height (in pixels).
    pub fn can_fit_in(&self, width: f64, height: f64) -> bool {
        let (w, h) = self.get_dimensions();

        w < width && h < height
    }

    /// Scales the bounding rectangle so that the view fits exactly within the target width and height.
    /// Width and height are in pixels.
    pub fn scale_to_width_height(self, target_width: f64, target_height: f64) -> Self {
        let (width, height) = self.get_dimensions();

        let x_factor = target_width / width;
        let y_factor = target_height / height;

        let origin = self.bounding_rect.center();
        let scaled_br = self
            .bounding_rect
            .scale_around_point(x_factor, y_factor, origin);

        View::new(self.zoom, scaled_br)
    }

    /// Returns the minimum and maximum tile coordinates (as `(x, y)` tuples) covered by the view.
    pub fn min_max(&self) -> ((f64, f64), (f64, f64)) {
        let min = self
            .zoom
            .lonlat2tile(self.bounding_rect.min().x, self.bounding_rect.max().y);
        let max = self
            .zoom
            .lonlat2tile(self.bounding_rect.max().x, self.bounding_rect.min().y);
        (min, max)
    }
}

/// Creates a `View` from a bounding rectangle and desired pixel dimensions.
/// Automatically selects the highest zoom level that fits the area into the given size,
/// then scales the bounding rectangle to fit exactly.
impl From<(Rect, f64, f64)> for View {
    fn from((rect, width, height): (Rect, f64, f64)) -> Self {
        let mut map = View::new(Zoom::new(18), rect);

        for i in (1..=17).rev() {
            if map.can_fit_in(width, height) {
                break;
            }
            map = View::new(Zoom::new(i), rect);
        }

        map.scale_to_width_height(width, height)
    }
}

/// Represents a zoom level for map tile calculations.
#[derive(Clone)]
struct Zoom {
    zoom: u8,
}

impl Zoom {
    pub fn new(zoom: u8) -> Self {
        Self { zoom }
    }

    /// Converts longitude and latitude to pixel coordinates at the current zoom level.
    pub fn lonlat2xy(&self, lon: f64, lat: f64) -> (f64, f64) {
        let tile = self.lonlat2tile(lon, lat);
        (tile.0 * 256., tile.1 * 256.)
    }

    /// Converts longitude and latitude to tile coordinates at the current zoom level.
    pub fn lonlat2tile(&self, lon: f64, lat: f64) -> (f64, f64) {
        let lat_rad = lat.to_radians();
        let zz: f64 = 2f64.powf(self.zoom as f64);
        let x: f64 = (lon + 180f64) / 360f64 * zz;
        let y: f64 = (1f64 - (lat_rad.tan() + (1f64 / lat_rad.cos())).ln() / PI) / 2f64 * zz;
        (x, y)
    }
}

struct Map {
    view: View,
    pixmap: Pixmap,
}

impl Map {
    pub fn new(view: View) -> Self {
        let (width, height) = view.get_dimensions();
        let pixmap =
            Pixmap::new(width as u32, height as u32).expect("dimensions are greater than zero");
        Self { view, pixmap }
    }

    async fn fill_tiles<F, Fut>(
        &mut self,
        getter: F,
        base: Vec<Arc<str>>,
    ) -> Result<(), error::GPXError>
    where
        F: Fn(String) -> Pin<Box<Fut>>,
        Fut: Future<Output = Result<(String, Vec<u8>), String>> + 'static + Send,
    {
        let zoom: u8 = self.view.zoom.zoom;
        let (min, max) = self.view.min_max();
        let mut set = JoinSet::new();
        let mut ctx: HashMap<String, (i32, i32)> = HashMap::new();

        let mut iter = (min.0 as i32..=max.0 as i32 + 1).flat_map(|x_tile| {
            (min.1 as i32..=max.1 as i32 + 1).map(move |y_tile| (x_tile, y_tile))
        });

        // two request in parallel.
        for (x_tile, y_tile) in iter.by_ref().take(2) {
            let url = format!(
                "{}/{}/{}/{}.png",
                base[((x_tile + y_tile) % base.len() as i32) as usize].trim_start_matches("/"),
                zoom,
                x_tile,
                y_tile
            );

            ctx.insert(url.clone(), (x_tile, y_tile));

            set.spawn((getter)(url));
        }

        while let Some(res) = set.join_next().await {
            let (url, bytes) = res?.map_err(GPXError::TileFetch)?;
            let bytes = Pixmap::decode_png(&bytes).expect("Tile should be loaded as Pixmap");
            let (x_tile, y_tile) = ctx.get(&url).expect("context should exist");

            let x_offset = (*x_tile as f64 - min.0) * 256.;
            let y_offset = (*y_tile as f64 - min.1) * 256.;

            self.pixmap.draw_pixmap(
                x_offset as i32,
                y_offset as i32,
                bytes.as_ref(),
                &PixmapPaint::default(),
                Transform::default(),
                None,
            );

            if let Some((x_tile, y_tile)) = iter.next() {
                let url = format!(
                    "{}/{}/{}/{}.png",
                    base[((x_tile + y_tile) % base.len() as i32) as usize],
                    zoom,
                    x_tile,
                    y_tile
                );

                ctx.insert(url.clone(), (x_tile, y_tile));
                set.spawn((getter)(url));
            }
        }

        Ok(())
    }

    pub fn plot_path<I: IntoIterator<Item = Point>>(&mut self, points: I) {
        let (min, _) = self.view.min_max();
        let mut pb = PathBuilder::new();
        let min_tile_x = min.0;
        let min_tile_y = min.1;
        let min_xy = (min_tile_x * 256., min_tile_y * 256.);
        let mut iter = points.into_iter();

        if let Some(p) = iter.next() {
            let p_xy = self.view.zoom.lonlat2xy(p.x(), p.y());
            pb.move_to(
                (p_xy.0 - min_xy.0).floor() as f32,
                (p_xy.1 - min_xy.1).floor() as f32,
            );
        }

        for p in iter {
            let p_xy = self.view.zoom.lonlat2xy(p.x(), p.y());

            pb.line_to(
                (p_xy.0 - min_xy.0).floor() as f32,
                (p_xy.1 - min_xy.1).floor() as f32,
            );
        }

        let path = pb.finish().expect("path should be built");

        let stroke = Stroke {
            width: 10.,
            miter_limit: 10.,
            line_cap: LineCap::Round,
            line_join: LineJoin::default(),
            dash: StrokeDash::new(vec![80.0, 20.0], 0.0),
        };

        let paint = Paint {
            shader: Shader::SolidColor(Color::from_rgba8(255, 0, 0, 155)),
            ..Default::default()
        };

        self.pixmap.stroke_path(
            &path,
            &paint,
            &stroke,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    pub fn add_attribution(&mut self, attribution_path: PathBuf) {
        let attribution_pixmap =
            Pixmap::load_png(&attribution_path).expect("Attribution image should be loaded");

        let attribution_pixmap = if attribution_pixmap.width() > self.pixmap.width() {
            let scale_factor = self.pixmap.width() as f32 / attribution_pixmap.width() as f32;
            let scaled_width = (attribution_pixmap.width() as f32 * scale_factor) as u32;
            let scaled_height = (attribution_pixmap.height() as f32 * scale_factor) as u32;
            let mut scaled_pixmap =
                Pixmap::new(scaled_width, scaled_height).expect("Scaled pixmap should be created");

            scaled_pixmap.draw_pixmap(
                0,
                0,
                attribution_pixmap.as_ref(),
                &PixmapPaint::default(),
                Transform::from_scale(scale_factor, scale_factor),
                None,
            );
            scaled_pixmap
        } else {
            attribution_pixmap
        };

        self.pixmap.draw_pixmap(
            (self.pixmap.width() - attribution_pixmap.width()) as i32,
            (self.pixmap.height() - attribution_pixmap.height()) as i32,
            attribution_pixmap.as_ref(),
            &PixmapPaint::default(),
            Transform::default(),
            None,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lonlat2tile_known_values() {
        let zoom = Zoom::new(8);
        let (x, y) = zoom.lonlat2tile(48.1372, 11.5761);
        assert_eq!(162.23089777777778, x);
        assert_eq!(119.71152313727555, y);
    }

    #[test]
    fn test_lonlat2xy_known_values() {
        let zoom = Zoom::new(5);
        let (x, y) = zoom.lonlat2xy(48.1372, 11.5761);
        assert_eq!(5191.388728888889, x);
        assert_eq!(3830.7687403928176, y);
    }

    #[test]
    fn test_view_new_and_get_dimensions() {
        let rect = Rect::new(Point::new(0.0, 0.0), Point::new(2.0, 2.0));
        let view = View::new(Zoom::new(2), rect);
        assert_eq!(
            view.get_dimensions(),
            (5.688888888888869, 5.690044530704711)
        );
    }

    #[test]
    fn test_view_can_fit_in() {
        let rect = Rect::new(Point::new(0.0, 0.0), Point::new(2.0, 2.0));
        let view = View::new(Zoom::new(2), rect);
        assert!(view.can_fit_in(6., 6.));
        assert!(!view.can_fit_in(5., 5.));
    }
}
