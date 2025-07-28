use std::{fs::File, path::PathBuf};

use gpx::{Gpx, Time, Track, Waypoint};

use crate::{
    GPXError,
    utils::{Line, read_gpx_file},
};

#[derive(clap::Args)]
pub struct PathsArgs {
    /// Input file paths use, can be specified multiple times
    #[clap(required = true, short, long)]
    input: Vec<PathBuf>,
    /// Output file path
    #[clap(short, long)]
    output: PathBuf,
    /// Overwrite output file if it exists
    #[clap(long, short, default_value_t = false)]
    force: bool,
}

struct TrackData {
    gpx: Gpx,
    time: Time,
}

pub fn join(paths: PathsArgs) -> Result<(), GPXError> {
    let mut tracks_data: Vec<TrackData> = Vec::new();
    for path in paths.input {
        let gpx: Gpx = read_gpx_file(&path)?;
        let mut last_waypoint: Option<Waypoint> = None;
        for point in Line::new(&gpx.tracks) {
            if let Some(last) = &last_waypoint {
                if last.time.expect("waypoint to have time")
                    < point.time.expect("waypoint to have time")
                {
                    last_waypoint = Some(point.clone());
                }
            } else {
                last_waypoint = Some(point.clone());
            }
        }

        tracks_data.push(TrackData {
            gpx,
            time: last_waypoint
                .as_ref()
                .expect("file to have at least one waypoint")
                .time
                .expect("waypoint to have time"),
        });
    }
    tracks_data.sort_by(|a, b| a.time.cmp(&b.time));
    let mut joined_gpx = Gpx {
        version: gpx::GpxVersion::Gpx11,
        creator: Some("gpxtools".to_string()),
        metadata: tracks_data.first().and_then(|d| d.gpx.metadata.clone()),
        tracks: vec![Track {
            name: Some(
                tracks_data
                    .first()
                    .and_then(|d| d.gpx.tracks.first().and_then(|t| t.name.clone()))
                    .unwrap_or("Joined Track".to_string())
                    .to_string(),
            ),
            segments: Vec::new(),
            ..Default::default()
        }],
        ..Default::default()
    };
    for track_data in tracks_data {
        let t = joined_gpx.tracks.last_mut().expect("at least one track");
        t.segments
            .extend(track_data.gpx.tracks.into_iter().flat_map(|tr| tr.segments));
    }
    if paths.output.exists() && !paths.force {
        panic!(
            "Output file {:?} already exists. Use --force to overwrite.",
            paths.output
        );
    }

    let output_file = File::create(&paths.output).expect("Failed to create output file");
    gpx::write(&joined_gpx, output_file).expect("Failed to write GPX file");
    println!("Joined GPX file written to {:?}", paths.output);

    Ok(())
}
