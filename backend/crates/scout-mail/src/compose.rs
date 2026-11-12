use anyhow::{Context, Result};
use lettre::Message;
use lettre::message::Mailbox;
use lettre::message::header::ContentType;

const SUBJECT_PREFIX: &str = "Subject:";

#[derive(Debug)]
pub struct Slots<'a> {
    pub subject: &'a str,
    pub first_name: &'a str,
    pub hook: &'a str,
    pub proof: &'a str,
    pub offer: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Email {
    pub subject: String,
    pub body: String,
}

/// Fills the template. Its first line is the subject, the rest is the body.
pub fn render(template: &str, slots: &Slots<'_>) -> Result<Email> {
    let filled = template
        .replace("{{subject}}", slots.subject)
        .replace("{{first_name}}", slots.first_name)
        .replace("{{hook}}", slots.hook)
        .replace("{{proof}}", slots.proof)
        .replace("{{offer}}", slots.offer)
        .replace("\r\n", "\n");
    let (first, body) = filled.split_once('\n').context("Template has no body")?;
    let subject = first
        .strip_prefix(SUBJECT_PREFIX)
        .context("Template must start with a Subject: line")?;
    Ok(Email { subject: subject.trim().to_owned(), body: body.trim().to_owned() })
}

/// The raw message for the Drafts folder, as plain text.
pub fn build_message(
    from_name: &str,
    from: &str,
    to: &str,
    email: &Email,
    message_id: &str,
) -> Result<Vec<u8>> {
    let sender = Mailbox::new(Some(from_name.to_owned()), from.parse()?);
    let message = Message::builder()
        .from(sender)
        .to(to.parse::<Mailbox>()?)
        .subject(&email.subject)
        .message_id(Some(format!("<{message_id}>")))
        .date_now()
        .header(ContentType::TEXT_PLAIN)
        .body(email.body.clone())?;
    Ok(message.formatted())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slots() -> Slots<'static> {
        Slots {
            subject: "a question",
            first_name: "Ada",
            hook: "Saw the raise.",
            proof: "Built X.",
            offer: "I could build Y.",
        }
    }

    #[test]
    fn fills_every_slot() {
        let template = "Subject: {{subject}}\r\n\r\nHey {{first_name}},\n{{hook}}\n{{proof}}\n{{offer}}\n";
        let email = render(template, &slots()).unwrap();
        assert_eq!(email.subject, "a question");
        assert_eq!(email.body, "Hey Ada,\nSaw the raise.\nBuilt X.\nI could build Y.");
    }

    #[test]
    fn refuses_a_template_without_a_subject() {
        assert!(render("Hey {{first_name}},\nbody", &slots()).is_err());
    }

    #[test]
    fn builds_a_plain_text_message() {
        let email = Email { subject: "hi".into(), body: "Hello".into() };
        let raw = build_message("V N", "v@gmail.com", "ada@acme.ai", &email, "1@scout").unwrap();
        let raw = String::from_utf8(raw).unwrap();
        assert!(raw.contains("To: ada@acme.ai"));
        assert!(raw.contains("Message-ID: <1@scout>"));
        assert!(raw.contains("text/plain"));
    }
}
