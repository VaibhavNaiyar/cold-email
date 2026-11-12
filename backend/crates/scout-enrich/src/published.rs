/// An address found on a public page, with that page as proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    pub email: String,
    pub source_type: &'static str,
    pub source_url: String,
}
