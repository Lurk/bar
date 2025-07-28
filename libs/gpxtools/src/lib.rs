pub mod error;
pub mod join;
pub mod plot;
pub mod stats;

mod utils;

pub use crate::plot::{PlotArgs, plot};

pub use crate::stats::{StatsArgs, calculate_stats};

pub use crate::join::{PathsArgs, join};

pub use crate::error::GPXError;
