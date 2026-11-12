use crate::Posting;

/// Keeps roles whose title and place fit the profile. Matching is by whole words.
#[derive(Debug)]
pub struct RoleFilter {
    include: Vec<String>,
    exclude: Vec<String>,
    places: Vec<String>,
}

impl RoleFilter {
    pub fn new(include: &[String], exclude: &[String], places: &[String]) -> Self {
        let words = |list: &[String]| list.iter().map(|item| padded(item)).collect();
        Self { include: words(include), exclude: words(exclude), places: words(places) }
    }

    pub fn keeps(&self, posting: &Posting) -> bool {
        self.title_fits(&posting.title) && self.place_fits(posting)
    }

    fn title_fits(&self, title: &str) -> bool {
        let title = padded(title);
        self.include.iter().any(|word| title.contains(word.as_str()))
            && !self.exclude.iter().any(|word| title.contains(word.as_str()))
    }

    // Remote works from anywhere; an office must be in one of the profile's places.
    // An unstated location is kept so the scorer can judge it.
    fn place_fits(&self, posting: &Posting) -> bool {
        if posting.remote || posting.location.trim().is_empty() {
            return true;
        }
        let location = padded(&posting.location);
        self.places.iter().any(|place| location.contains(place.as_str()))
    }
}

/// Lower case, punctuation as spaces, wrapped in spaces so `contains` matches whole words.
fn padded(text: &str) -> String {
    let words: Vec<String> = text
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect();
    format!(" {} ", words.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    fn filter() -> RoleFilter {
        RoleFilter::new(
            &strings(&["engineer", "developer", "intern", "sde", "full-stack"]),
            &strings(&["senior", "sr", "staff", "manager", "sales", "iii"]),
            &strings(&["India", "Bengaluru"]),
        )
    }

    fn posting(title: &str, location: &str, remote: bool) -> Posting {
        Posting {
            external_id: "1".into(),
            title: title.into(),
            company: None,
            location: location.into(),
            remote,
            department: None,
            url: "https://x".into(),
            posted_at: None,
            description: None,
            notes: None,
        }
    }

    #[test]
    fn keeps_matching_titles_and_drops_senior_ones() {
        let filter = filter();
        assert!(filter.keeps(&posting("Founding AI Engineer", "", false)));
        assert!(filter.keeps(&posting("SDE Intern", "", false)));
        assert!(filter.keeps(&posting("Full Stack Developer", "", false)));
        assert!(!filter.keeps(&posting("Sr. Software Engineer", "", false)));
        assert!(!filter.keeps(&posting("Software Engineer III", "", false)));
        assert!(!filter.keeps(&posting("Sales Engineer", "", false)));
        assert!(!filter.keeps(&posting("Internal Auditor", "", false)), "intern is a whole word");
    }

    #[test]
    fn office_roles_must_be_in_a_listed_place() {
        let filter = filter();
        assert!(filter.keeps(&posting("Engineer", "Bengaluru, Karnataka", false)));
        assert!(filter.keeps(&posting("Engineer", "San Francisco", true)));
        assert!(!filter.keeps(&posting("Engineer", "San Francisco, CA", false)));
        assert!(!filter.keeps(&posting("Engineer", "Indianapolis", false)), "India is a whole word");
    }
}
