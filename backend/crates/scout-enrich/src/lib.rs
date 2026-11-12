//! Reads public sources for founders and published email addresses.

mod crawl;
mod emails;
mod founders;
mod hn;
mod homepage;
mod published;
mod search;

pub use crawl::{Page, read_pages, read_site};
pub use emails::{Judged, Person, Rank, extract_emails, judge};
pub use founders::{person, prioritise, yc_data_page, yc_founders};
pub use hn::hn_emails;
pub use homepage::{homepage_text, html_text};
pub use published::Published;
pub use search::{search_emails, search_urls};
