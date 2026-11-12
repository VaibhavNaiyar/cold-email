//! Signal sources and domain resolution.

pub mod domain;
pub mod formd;
mod model;
pub mod resolver;
pub mod rss;
pub mod yc;

pub use model::{RawSignal, SignalKind, SourceType};
