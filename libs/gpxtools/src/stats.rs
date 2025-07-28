use std::path::PathBuf;

use geo::{Distance, Haversine};
use tabled::Tabled;

use crate::{
    GPXError,
    utils::{Line, read_gpx_file},
};

#[derive(clap::Args)]
pub struct StatsArgs {
    /// Input file path
    #[clap(short, long)]
    input: Vec<PathBuf>,
}

#[derive(Tabled)]
pub struct Stats {
    #[tabled(rename = "File")]
    pub file: String,
    #[tabled(rename = "Distance", format("{:.2} km"))]
    pub distance_km: f64,
    #[tabled(rename = "Ascend", format("{:.0} m"))]
    pub total_ascent_m: f64,
}

pub fn calculate_stats(stats: StatsArgs) -> Result<Vec<Stats>, GPXError> {
    let mut results = Vec::new();
    for input in &stats.input {
        let gpx = read_gpx_file(input)?;
        let mut iter = Line::new(&gpx.tracks);
        let mut distance_traveled_km = 0.0;
        let mut total_ascent_m = 0.0;

        if let Some(mut prev) = iter.next() {
            for point in Line::new(&gpx.tracks) {
                distance_traveled_km += Haversine.distance(prev.point(), point.point()) / 1000.0;
                if let Some(elev) = point.elevation
                    && let Some(prev_elev) = prev.elevation
                    && elev > prev_elev
                {
                    total_ascent_m += elev - prev_elev;
                }

                prev = point;
            }
        }

        results.push({
            Stats {
                file: input.display().to_string(),
                distance_km: distance_traveled_km,
                total_ascent_m,
            }
        });
    }

    Ok(results)
}
