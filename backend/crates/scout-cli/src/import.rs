use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result};
use scout_db::{NewBoard, Pool};
use scout_jobs::Ats;
use serde_json::{Value, json};

/// Adds the careers boards found in a list of links, one per line: `URL`, or `Name<TAB>URL`.
/// Lines whose links are not on a known board host are counted and skipped.
pub async fn run(pool: &Pool, path: &Path) -> Result<Value> {
    let text = std::fs::read_to_string(path).with_context(|| format!("Could not read {}", path.display()))?;
    let mut seen: BTreeSet<(Ats, String)> = BTreeSet::new();
    let (mut added, mut known, mut skipped) = (0, 0, 0);

    for line in text.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#')) {
        let (name, link) = line.split_once('\t').map_or((None, line), |(name, link)| (Some(name.trim()), link));
        let boards = scout_jobs::find_boards(link);
        if boards.is_empty() {
            skipped += 1;
        }
        for (ats, slug) in boards {
            if !seen.insert((ats, slug.clone())) {
                continue;
            }
            let fallback = crate::jobs::board_name(&slug);
            let board = NewBoard {
                company_id: None,
                name: name.filter(|name| !name.is_empty()).unwrap_or(&fallback),
                ats: ats.as_str(),
                slug: &slug,
                source: "manual",
            };
            if scout_db::upsert_board(pool, &board).await? {
                added += 1;
            } else {
                known += 1;
            }
        }
    }
    Ok(json!({ "boards_added": added, "already_known": known, "lines_without_a_board": skipped }))
}
