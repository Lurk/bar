use std::{collections::HashMap, f64::consts::PI, path::PathBuf, pin::Pin, sync::Arc};

use geo::{Point, Rect, Scale};
use tiny_skia::{
    Color, LineCap, LineJoin, Paint, PathBuilder, Pixmap, PixmapPaint, Shader, Stroke, StrokeDash,
    Transform,
};
use tokio::task::JoinSet;

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
    pub copyright: Option<PathBuf>,
    /// Overwrite output file if it exists
    #[clap(long, short, default_value_t = false)]
    pub force: bool,
}

pub async fn plot<F, Fut>(getter: F, plot: PlotArgs) -> Result<(), error::GPXError>
where
    Fut: Future<Output = Result<(String, Vec<u8>), String>> + 'static + Send,
    F: Fn(String) -> Pin<Box<Fut>>,
{
    let gpx = read_gpx_file(&plot.input)?;
    let br = Line::new(&gpx.tracks)
        .get_bounding_rect()
        .expect("bounding rect exists");

    let mut map = Map::new(View::from((br, plot.width, plot.height)));

    map.fill_tiles(&getter, plot.base).await?;
    map.plot_path(Line::new(&gpx.tracks).map(|wpt| wpt.point()));

    if let Some(copyright_path) = plot.copyright {
        map.add_copyright(copyright_path);
    }

    map.pixmap
        .save_png(plot.output)
        .expect("Pixmap should be saved");

    Ok(())
}

struct View {
    zoom: Zoom,
    bounding_rect: Rect,
}

impl View {
    fn new(zoom: Zoom, bounding_rect: Rect) -> Self {
        Self {
            zoom,
            bounding_rect,
        }
    }

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

    pub fn can_fit_in_square(&self, width: f64, height: f64) -> bool {
        let (w, h) = self.get_dimensions();

        w < width && h < height
    }

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

impl From<(Rect, f64, f64)> for View {
    fn from((rect, width, height): (Rect, f64, f64)) -> Self {
        let mut map = View::new(Zoom::new(18), rect);

        for i in (1..=17).rev() {
            if map.can_fit_in_square(width, height) {
                break;
            }
            map = View::new(Zoom::new(i), rect);
        }

        map.scale_to_width_height(width, height)
    }
}

#[derive(Clone)]
struct Zoom {
    zoom: u8,
}

impl Zoom {
    pub fn new(zoom: u8) -> Self {
        Self { zoom }
    }

    pub fn lonlat2xy(&self, lon: f64, lat: f64) -> (f64, f64) {
        let tile = self.lonlat2tile(lon, lat);
        (tile.0 * 256., tile.1 * 256.)
    }

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

        for (x_tile, y_tile) in iter.by_ref().take(5) {
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

    pub fn add_copyright(&mut self, copyright_path: PathBuf) {
        let copyright_pixmap =
            Pixmap::load_png(&copyright_path).expect("Copyright image should be loaded");

        let x_offset = self.pixmap.width() as i32 - copyright_pixmap.width() as i32;
        let y_offset = self.pixmap.height() as i32 - copyright_pixmap.height() as i32;

        self.pixmap.draw_pixmap(
            x_offset,
            y_offset,
            copyright_pixmap.as_ref(),
            &PixmapPaint::default(),
            Transform::default(),
            None,
        );
    }
}
