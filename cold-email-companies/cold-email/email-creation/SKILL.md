---
name: cold-email
description: Write researched, personalized cold emails to tech companies from the compiled leads workbook (company-list/only-tech/Tech_Company_Leads_Compiled.xlsx), using the Cold Email Hall of Fame method (Golden Formula, curiosity-gap subject lines, value-first, Yes/No/Forward ask, P.S., follow-ups). Use when the user asks to write, draft, batch or follow up on cold emails to companies, founders or recruiters for jobs/internships, or names a company from the leads list.
---

# Cold email creation

Writes one cold email per company: research the company → find a real hook →
write with the Golden Formula → self-check → save the draft and mark the workbook.

## Files

| Path (relative to this folder) | What it is |
|---|---|
| `profile.md` | The sender: story, skills, proof, links. **Every email draws credibility from here.** |
| `references/playbook.md` | The method, distilled from the 8-day course + newsletters, with the real emails that worked (verbatim) and why. Read before writing. |
| `references/templates.md` | Skeletons for each lead type (startup founder, HN hiring email, visa list, big company) + follow-ups. |
| `references/research.md` | How to research a company and what to pull out. |
| `scripts/pick_companies.py` | Selects leads from the workbook (filters, skips already-drafted). |
| `scripts/save_email.py` | Saves a draft to `drafts/` and writes "Drafted <date>" into the workbook's *Email status* column. |
| `drafts/` | Output. One `.md` per email + `drafts_log.csv` (for mail-merge / tracking). |

Workbook: `E:\AI-JOB\cold-email-companies\company-list\only-tech\Tech_Company_Leads_Compiled.xlsx`
Original course docs: `E:\AI-JOB\cold-email-companies\cold-email\refrence\word-docs\`

## Workflow

### 0. Check the profile (once per session)
Read `profile.md`. If it still contains `TODO` placeholders, stop and ask the user for the
missing facts (name, 1–2 line story, best 2–3 proof points with numbers, links). Never invent
credentials, projects, metrics or experience. A weak true fact beats an impressive fake one.

### 1. Pick the companies
```
python scripts/pick_companies.py --tab "Best for you" --limit 5 --with-email
```
Useful flags: `--tab` (Best for you | Remote | Hybrid-Flex | On-site | H-1B Sponsors),
`--region India`, `--entry-only`, `--with-email`, `--company "Name"` (exact lookup),
`--include-drafted`. Output is JSON (company, role, location, website, job link, contact email/name, notes…).
If the user names companies, use `--company` for each. Default: 5 per batch.

### 2. Research each company (do not skip — this is what makes the email work)
Follow `references/research.md`. Minimum per company, using WebSearch / WebFetch:
1. The **job link** and **website** from the row — what they build, for whom, stage.
2. **Who to email**: the founder / CTO / hiring manager by name. Use the row's contact if present;
   otherwise find founders on the YC page / About page / LinkedIn. Never guess an email address
   pattern and present it as known — mark guessed addresses `(unverified)`.
3. **One specific hook** that only someone who looked would know: a recent launch, blog post,
   changelog item, founder tweet/podcast, funding news, open-source repo, a product quirk.
4. **A value idea**: something small and concrete the sender can give first — a bug found, a
   feature idea tied to their product, a quick prototype/teardown, a doc fix, a PR.
Record sources (URLs) for each fact; put them in the draft's notes.
If research turns up nothing specific, say so in the notes and use the product itself as the hook.

### 3. Write the email
Read `references/playbook.md` and pick the matching skeleton from `references/templates.md`.
Hard rules:
- **Subject**: curiosity gap, 2–8 words, lowercase-casual is fine, never "Job application" / "Inquiry".
  Write 3 options, pick the best, keep the other two in the notes.
- **Opener**: like a text to a friend — "Hey {first name}," / "{Name} - " / "Hey {Name}, what's up?"
  Never "Dear Sir/Madam", "Respected", "Hope this email finds you well", "I am writing to".
- **Body**: Golden Formula in order — Attention → Personal touch → Credibility → Offer value → Simple ask.
  75–150 words. Sentences short. One idea per line. Grade 5–8 reading level. Active voice.
  Bold (`**…**`) at most 1–2 key phrases so a skimmer gets the point.
- **Ask**: passes the 3-Test Rule — the reader can answer Yes / No / Forward in one word.
  e.g. "Worth a 15-min chat? Just reply 'yes'." / "Want me to send it over?" / "Who's the right person to send this to?"
  Never "let me know if there are any openings", "pick your brain", "attached is my resume for your consideration".
- **P.S.**: always. One line — humour, a second personal detail, or a link to proof.
- Sign-off: "Best," / "Cheers," + first name. Links (GitHub / portfolio / resume) go in P.S. or signature, not the opener.

### 4. Self-check before saving (fix and rewrite if any fails)
- [ ] Could this email be sent to a different company unchanged? → if yes, not personal enough.
- [ ] Does the first line earn the second? Would a busy founder on coffee #6 keep reading?
- [ ] Is there something given before something asked?
- [ ] Is the ask answerable in one word?
- [ ] ≤150 words in the body? No paragraph over 3 lines?
- [ ] Every claim about the sender is in `profile.md`; every claim about the company has a source.
- [ ] No buzzwords: "passionate", "synergy", "leverage", "revolutionize", "I believe I'd be a great fit".

### 5. Save
```
python scripts/save_email.py --company "Acme" --role "Founding Engineer" --to "jane@acme.com" \
  --to-name "Jane Doe" --subject "the bug in your onboarding" --body-file <tmp.md> \
  --notes-file <notes.md> --followups-file <followups.md>
```
Body file = the email exactly as it would be sent. Notes file = research summary + sources + alt subjects.
Follow-ups file = the 3 follow-ups below. The script writes `drafts/<date>_<company>.md`,
appends `drafts/drafts_log.csv`, and marks *Email status* = "Drafted <date>" in every tab of the
workbook where the company appears (warns instead of failing if the workbook is open in Excel).

### 6. Follow-ups (write with every email)
Per the playbook (follow-ups double reply rate; always give an easy out):
1. **Day 3–4** — bump with progress or new value ("shipped the prototype I mentioned: <link>").
2. **Day 7–8** — handle the likely objection or add a second freebie.
3. **Day 14** — short, human, easy out: "if this isn't a priority right now, no worries — still a fan of what you're building."
Each 2–4 lines, same thread (reply to the original, keep the subject).

### 7. Report back
Show the user each email (subject + body) in chat, the recipient and how confident you are in the
address, and the one research fact the email hinges on. Do not send anything. If the user explicitly
asks to put them in Gmail, create **drafts** (not sends) with the Gmail tools, one per email.

## Batch mode
For "write emails for 10 companies": pick with the script, then research + write + save one company at a
time (quality over speed — a generic email to 10 people is worse than 3 great ones). Summarise at the end
as a table: company · recipient · subject · hook used · draft file.
