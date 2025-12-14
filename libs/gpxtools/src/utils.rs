use std::{fs::File, io::BufReader, path::PathBuf};

use geo::{Coord, Rect};
use gpx::{Gpx, Track, Waypoint};

use crate::GPXError;

/// Reads a GPX file from the specified path and returns a parsed `Gpx` object.
///
/// # Example
/// ```ignore
/// let path = PathBuf::from("track.gpx");
/// let gpx = read_gpx_file(&path)?;
/// ```
pub fn read_gpx_file(path: &PathBuf) -> Result<Gpx, GPXError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    Ok(gpx::read(reader)?)
}

/// An iterator for traversing all waypoints in a collection of GPX tracks.
///
/// The `Line` struct provides a convenient way to iterate over every `Waypoint`
/// in a vector of `Track` objects, traversing through all tracks, their segments,
/// and the points within each segment in order.
///
/// # Example
/// ```ignore
/// let line = Line::new(&tracks);
/// for waypoint in line {
///     // process waypoint
/// }
/// ```
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

    /// Calculates the bounding rectangle that contains all waypoints in the tracks.
    ///
    /// Iterates through all waypoints in the `Line` and determines the minimum and maximum
    /// x (longitude) and y (latitude) coordinates.
    ///
    /// # Example
    /// ```ignore
    /// let bounding_rect = Line::new(&tracks).get_bounding_rect();
    /// if let Some(rect) = bounding_rect {
    ///     // use rect
    /// }
    /// ```
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

/// Updates a minimum and maximum value tuple with a new value.
///
/// Given a value `v` and the current minimum (`min`) and maximum (`max`),
/// returns a new tuple where:
/// - If `v` is greater than `max`, the tuple becomes (`min`, `v`)
/// - If `v` is less than `min`, the tuple becomes (`v`, `max`)
/// - Otherwise, the tuple remains (`min`, `max`)
///
/// # Arguments
///
/// * `v` - The new value to compare.
/// * `min` - The current minimum value.
/// * `max` - The current maximum value.
///
/// # Returns
///
/// A tuple `(f64, f64)` representing the updated minimum and maximum values.
///
/// # Examples
///
/// ```ignore
/// let (min, max) = min_max(5.0, 2.0, 4.0);
/// assert_eq!((min, max), (2.0, 5.0));
/// ```
pub fn min_max(v: f64, min: f64, max: f64) -> (f64, f64) {
    if v > max {
        (min, v)
    } else if v < min {
        (v, max)
    } else {
        (min, max)
    }
}

#[cfg(test)]
mod tests {
    use super::{Line, min_max};
    use geo::Point;
    use gpx::{Track, TrackSegment, Waypoint};

    fn waypoint(lat: f64, lon: f64) -> Waypoint {
        let point = Point::new(lon, lat);
        Waypoint::new(point)
    }

    fn segment_with_points(points: Vec<Waypoint>) -> TrackSegment {
        let mut seg = TrackSegment::new();
        seg.points = points;
        seg
    }

    fn track_with_segments(segments: Vec<TrackSegment>) -> Track {
        let mut trk = Track::new();
        trk.segments = segments;
        trk
    }

    #[test]
    fn test_line_iterates_all_waypoints() {
        let seg = segment_with_points(vec![waypoint(1.0, 2.0), waypoint(3.0, 4.0)]);
        let seg2 = segment_with_points(vec![waypoint(5.0, 6.0), waypoint(7.0, 8.0)]);
        let trk = track_with_segments(vec![seg, seg2]);
        let tracks = vec![trk];
        let points: Vec<_> = Line::new(&tracks).collect();
        assert_eq!(points.len(), 4);
        assert_eq!(points[0].point().x_y(), (2.0, 1.0));
        assert_eq!(points[1].point().x_y(), (4.0, 3.0));
        assert_eq!(points[2].point().x_y(), (6.0, 5.0));
        assert_eq!(points[3].point().x_y(), (8.0, 7.0));
    }

    #[test]
    fn test_line_empty_tracks() {
        let tracks: Vec<Track> = vec![];
        let mut line = Line::new(&tracks);
        assert!(line.next().is_none());
        assert!(Line::new(&tracks).get_bounding_rect().is_none());
    }

    #[test]
    fn test_line_get_bounding_rect_single_point() {
        let tracks = vec![track_with_segments(vec![segment_with_points(vec![
            waypoint(5.0, 6.0),
        ])])];
        let rect = Line::new(&tracks).get_bounding_rect().unwrap();
        assert_eq!(rect.min().x, 6.0);
        assert_eq!(rect.min().y, 5.0);
        assert_eq!(rect.max().x, 6.0);
        assert_eq!(rect.max().y, 5.0);
    }

    #[test]
    fn test_line_get_bounding_rect_multiple_points() {
        let seg1 = segment_with_points(vec![waypoint(1.0, 2.0), waypoint(3.0, 4.0)]);
        let seg2 = segment_with_points(vec![waypoint(-1.0, 10.0)]);
        let trk = track_with_segments(vec![seg1, seg2]);
        let tracks = vec![trk];
        let rect = Line::new(&tracks).get_bounding_rect().unwrap();
        assert_eq!(rect.min().x, 2.0);
        assert_eq!(rect.max().x, 10.0);
        assert_eq!(rect.min().y, -1.0);
        assert_eq!(rect.max().y, 3.0);
    }

    #[test]
    fn test_min_max_greater_than_max() {
        assert_eq!(min_max(10.0, 2.0, 5.0), (2.0, 10.0));
    }

    #[test]
    fn test_min_max_less_than_min() {
        assert_eq!(min_max(1.0, 2.0, 5.0), (1.0, 5.0));
    }

    #[test]
    fn test_min_max_between_min_and_max() {
        assert_eq!(min_max(3.0, 2.0, 5.0), (2.0, 5.0));
    }

    #[test]
    fn test_min_max_equal_to_min() {
        assert_eq!(min_max(2.0, 2.0, 5.0), (2.0, 5.0));
    }

    #[test]
    fn test_min_max_equal_to_max() {
        assert_eq!(min_max(5.0, 2.0, 5.0), (2.0, 5.0));
    }
}
