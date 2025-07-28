use std::{fs::File, io::BufReader, path::PathBuf};

use geo::{Coord, Rect};
use gpx::{Gpx, Track, Waypoint};

use crate::GPXError;

pub fn read_gpx_file(path: &PathBuf) -> Result<Gpx, GPXError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    Ok(gpx::read(reader)?)
}

pub struct Line<'a> {
    tracks: &'a Vec<Track>,
    current_track: usize,
    current_segment: usize,
    current_point: usize,
}

impl<'a> Line<'a> {
    pub fn new(tracks: &'a Vec<Track>) -> Self {
        Line {
            tracks,
            current_track: 0,
            current_segment: 0,
            current_point: 0,
        }
    }

    pub fn get_bounding_rect(mut self) -> Option<Rect<f64>> {
        let waypoint = self.next()?;
        let (x, y) = waypoint.point().x_y();
        let mut x_min_max = (x, x);
        let mut y_min_max = (y, y);

        for waypoint in self {
            let (x, y) = waypoint.point().x_y();
            x_min_max = min_max(x, x_min_max.0, x_min_max.1);
            y_min_max = min_max(y, y_min_max.0, y_min_max.1);
        }

        Some(Rect::new(
            Coord {
                x: x_min_max.0,
                y: y_min_max.0,
            },
            Coord {
                x: x_min_max.1,
                y: y_min_max.1,
            },
        ))
    }
}

impl<'a> Iterator for Line<'a> {
    type Item = &'a Waypoint;

    fn next(&mut self) -> Option<Self::Item> {
        while self.current_track < self.tracks.len() {
            let track = &self.tracks[self.current_track];
            if self.current_segment < track.segments.len() {
                let segment = &track.segments[self.current_segment];
                if self.current_point < segment.points.len() {
                    let point = &segment.points[self.current_point];
                    self.current_point += 1;
                    return Some(point);
                } else {
                    self.current_segment += 1;
                    self.current_point = 0;
                }
            } else {
                self.current_track += 1;
                self.current_segment = 0;
                self.current_point = 0;
            }
        }
        None
    }
}

pub fn min_max(v: f64, min: f64, max: f64) -> (f64, f64) {
    if v > max {
        (min, v)
    } else if v < min {
        (v, max)
    } else {
        (min, max)
    }
}
