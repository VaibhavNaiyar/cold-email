//! Open roles from startup careers boards: Greenhouse, Lever, Ashby and YC.
//!
//! The first three publish public JSON APIs meant for this; YC pages embed the
//! same data and allow reading in robots.txt. Every link points at the
//! company's own careers board.

mod ats;
mod discover;
mod filter;

pub use ats::{Ats, Posting, fetch_board, fetch_description};
pub use discover::{BOARD_HOSTS, discover_boards, find_boards};
pub use filter::RoleFilter;
