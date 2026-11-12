# Startup Scout v2 — Automated Startup Outreach Engine

> Every morning: find newly funded AI startups, resolve their real domain, keep the ones that fit my profile, find a sourced founder email (technical founder first), fill my template, and put the finished email in my Gmail Drafts. I review it and press Send myself.
>
> Scope: personal use, ≤10 emails/day, public data and official APIs only. No chat bot. No LinkedIn/Wellfound scraping. No guessed emails.

**Status: approved; language changed to Rust.** The original TypeScript plan is kept in `PLAN.v1.md`. Telegram, WhatsApp, OAuth, HN, Product Hunt, ATS boards and follow-ups are out of scope here (section 9).

---

## 0. Rules for Claude Code

- Build one phase at a time; finish and test it before starting the next.
- Open the doc for an external API (section 10) before writing code against it. Do not invent fields.
- Rust stable, Cargo workspace, `cargo clippy` clean. Typed serde structs for every external response.
- There is no official Anthropic Rust SDK, so `scout-llm` calls the Messages API over HTTP with `reqwest`.
- Every adapter returns `Vec<RawSignal>`, is idempotent, and is safe to re-run.
- **No code path sends email.** The tool's only mail write is appending to Gmail Drafts. My click on Send in Gmail is the approval.
- Every stored address has `source_url` and `source_type`. No source, no row.
- Never store or use a pattern guess or an address from a data broker.
- Never take email addresses from GitHub (section 6).
- Respect rate limits and robots.txt. Use a descriptive `User-Agent` with my contact email on all HTTP calls.

---

## 1. Pipeline

```
Stage 1 Sources       RSS funding news · YC JSON · SEC Form D
        ▼
Stage 2 Ingest+Dedupe normalize → domain resolution → Postgres (companies, signals)
        ▼
Stage 3 Filter+Match  Haiku classification → hard filters → Sonnet profile scoring
        ▼
Stage 4 Email finder  founders (technical first) → website → Hacker News → web search
        ▼
Stage 5 Draft         template slot filling → lint → append to Gmail Drafts
        ▼
You                   review in Gmail → press Send (manual approval gate; Gmail does the sending)
        ▼
Sync                  next run reads Sent/Inbox over IMAP → updates status, suppression
```

## 2. Repo layout

The Rust workspace lives in `backend/`; run every `cargo` command from there. The web page lives in `frontend/` (section 8a).

```
backend/
Cargo.toml              # workspace, shared dependency versions
.cargo/config.toml      # `cargo scout <job>` alias
docker-compose.yml      # local Postgres (project name pinned to `ai-job`, keeping the data volume)
profile.yaml            # my skills, roles, locations
sources.yaml            # feed list, YC batches, daily caps
templates/outreach.md   # my template
templates/outreach-guide.md  # how the AI writes subject, hook and offer
crates/
  scout-cli/            # `scout` binary: migrate, ingest, match, contacts, jobs, draft, sync, suppress, run, serve
  scout-jobs/           # careers boards: Greenhouse, Lever, Ashby, YC; board discovery; title/place filter
  scout-config/         # .env, profile.yaml, sources.yaml
  scout-db/             # migrations/, models, queries, run logger
  scout-sources/        # model, domain, yc, rss, formd, resolver
  scout-llm/            # Claude HTTP client, prompts, JSON schemas
  scout-enrich/         # crawler, founder extraction, Hacker News and web search
  scout-mail/           # template renderer, linter, IMAP drafts + sync
```

## 3. Schema

SQL migrations in `crates/scout-db/migrations`, applied with `sqlx`. Postgres everywhere: Docker locally, Neon in production.

- `companies`: `domain` is nullable, and a company with no resolved domain never leaves Stage 2. `name_key` dedupes domain-less companies. Resolver state: `domain_source`, `domain_attempts`, `domain_last_attempt_at`.
- `contacts`: `is_technical`, `is_generic`, `provenance` (`page`). Multiple contacts per company. `source_type` and `source_url` are NOT NULL.
- `drafts`: `imap_uid`, `message_id`, `pushed_at`. Status: `pending, queued, skipped, sent, replied, bounced` (`queued` = in Gmail Drafts).
- `seen_urls`: articles already read, so non-funding items are not sent to the LLM twice.
- `suppression` and `runs`, with searches recorded in `runs.stats_json`.

## 4. Stage 1–2: sources and domain resolution

| Source | How | Domain? |
|---|---|---|
| RSS | `sources.yaml`: TechCrunch Venture + Startups, Sifted, Inc42, YourStory. A keyword prefilter drops non-funding items, then Haiku extracts `{company, domain?, round, amount, investors, founders[]}`. EU-Startups blocks bots (403) and is excluded; Entrackr's feed 404s | Sometimes |
| YC | `yc-oss` `meta.json` → latest N batch JSON files (active companies only). `isHiring` sets the signal kind. `hiring.json` is skipped: it is mostly old, large companies | Yes |
| SEC Form D | EDGAR daily index for `D` filings, parse `primary_doc.xml` for issuer, amount, related persons. Declared User-Agent, ≤10 req/s | Never |

**Form D prefilter (before any paid or capped call):** drop pooled funds, keep technology industry groups, skip amendments of known issuers. Daily filings far exceed the search quota otherwise.

**Domain resolution**
1. Normalize any known URL to its registrable domain with the `psl` crate (handles `acme.co.uk`; strips `www`, paths, shorteners, news sites).
2. If there is no domain, search `"{companyName}" official site` with **Tavily** (1,000 free credits/month, no card, 1 credit per basic search).
3. Haiku confirms the root domain from the results; reject aggregators such as Crunchbase, LinkedIn and news sites. Low confidence stays unresolved.
4. Cap at `max_domain_searches_per_day` (default 25, which stays within the free quota). Unresolved companies retry on later runs, up to 3 attempts.

Dedupe by domain, then upsert `signals` with `ON CONFLICT (source_url) DO NOTHING`.

## 5. Stage 3: filter and match

- Models are config values with these defaults: `LLM_FAST_MODEL=claude-haiku-5-5`, `LLM_SMART_MODEL=claude-sonnet-5-5`.
- Use Anthropic structured outputs (`output_config.format`) with hand-written JSON schemas, deserialized into serde structs.
- Fetch homepage text for classification, because Form D and short RSS items say too little.
- Classify: `{ is_ai, stage, remote_friendly|null, region, hiring_engineers|null, confidence }`.
- Hard filters: not suppressed, not contacted in 90 days, `is_ai`, stage and location match `profile.yaml`.
- Score: `{ score 0-100, reason ≤25 words, best_proof_point_index }`.
- Keep the top 15 for Stage 4.

## 6. Stage 4: email waterfall (free, published addresses only)

No paid finder: Hunter needs a business email to sign up. Every address must appear word for word on a public page, which is saved as its source, so nothing is guessed and no verifier is needed.

**Founders first.** Collect founders from the funding article, the YC company page (`data-page` JSON), Form D related persons, or the company website via Haiku. Technical founders are tried first.

**Sources, in order:**
1. **Website:** homepage, useful links from it, `sitemap.xml`, and the standard about, team, contact, careers, imprint, privacy and terms pages (12 pages at most). Reads `mailto:` links, plain text, `name [at] domain [dot] com`, and Cloudflare `data-cfemail`. 1 req/s per domain, 10 s timeout, respects robots.txt.
2. **Hacker News:** Algolia search for the domain across stories and comments, plus the profiles of those who submitted stories.
3. **Web search:** Tavily, only when no founder address was found, capped at `max_contact_searches_per_run`. Data brokers, LinkedIn and GitHub are excluded.

**Ranking:** technical founder, founder, founder inbox (`founders@`, `ceo@`), other named person, general inbox (`hello@`, `jobs@`). Inboxes such as `support@`, `privacy@`, `legal@` and `sales@` are never used. A private address (Gmail etc.) needs the founder's name in it.

**Volume:** the job walks down the ranking (score ≥ `min_contact_score`) until `max_emails_per_day × 2` companies have an address, instead of stopping at a fixed top 15.

**GitHub is excluded.** GitHub's Acceptable Use Policies prohibit using information from the service, including via the API, "for the purposes of sending unsolicited emails to users". This removes the "GitHub Commits" box from the original diagram.

## 7. Stage 5: draft and approval

- Render `templates/outreach.md`. The LLM fills only `subject`, `hook` and `offer`, following `templates/outreach-guide.md` (the Cold Email Hall of Fame method). `proof` is copied word for word from `profile.yaml`, and `first_name` comes from the stored contact ("{Company} team" for a shared inbox).
- The hook must cite the funding signal. Numbers and proper nouns not in the input are rejected. One rewrite is allowed, then the company waits for the next run.
- Lint: ≤120 words, sentences ≤20 words, subject ≤60 characters, no stock phrases, no claims of having used the product, no emojis, plain text, opt-out line present.
- Build the MIME message with `lettre` and append it to Gmail Drafts with `async-imap`, finding the folder by its `\Drafts` special-use flag rather than its name. Generate at most `max_emails_per_day` per run, and only one draft per company.
- Each run prints a summary: company, contact, source URL, score.
- **Sync job (each run):** IMAP search of Sent by recipient → `sent`. Search Inbox for mail from a contact → `replied`. Bounce notices → `bounced` + suppression. Any "no / not interested" reply is added to suppression manually with `cargo scout suppress <email>`.
- **Gmail auth:** App Password, which requires 2-Step Verification. It avoids the 7-day OAuth token expiry, but changing my Google password revokes it. Run fails loudly if so.

## 8. Build phases

| Phase | Deliverable | Done when |
|---|---|---|
| 1 ✅ | Cargo workspace, config, migrations, `runs` logging | `cargo scout migrate` works; `cargo scout ingest --dry` runs |
| 2 ✅ | RSS + YC + Form D adapters + domain resolver | Companies have clean domains; re-run inserts 0 duplicates; search cap holds |
| 3 ✅ | Classify + score | Top-15 printed with reasons; schema tests pass |
| 4 ✅ | Founder extraction + free waterfall | Every row has `source_url`; yield measured on real companies and reported |
| 5 (built; waiting on Gmail App Password for the live test) | Renderer, linter, IMAP drafts, sync | A test draft (to myself) appears in Gmail Drafts; nothing can send; sync marks it sent |
| 6 | Production: Neon + scheduled GitHub Action `30 3 * * *` (09:00 IST), private repo, failure alert via run status | Runs unattended for 7 days |

Tests: `cargo test`, using saved API responses as fixtures so unit tests stay off the network. Database tests use `#[sqlx::test]` against local Postgres.

## 8a. Web page (frontend v1)

One page that shows every scored company and the email it would get. Sign-in, sign-up and payments come later and are not built yet.

**Architecture.** The Rust binary serves a small JSON API (`cargo scout serve`, port 8787, bound to localhost only because it has no sign-in yet). The Next.js app (`frontend/`, Next 16.4, App Router, Tailwind 4) reads it on the server and proxies `/api/*` to it with a rewrite, so the browser never talks to Rust directly. The email writer is the same code the daily job uses, so the page and Gmail Drafts can't drift apart.

| Endpoint | Returns |
|---|---|
| `GET /api/companies` | Every scored AI company: what it does, stage, region, fit score and reason, funding (amount, round, investors, YC batch, Form D), source links, best address with its source, and any draft with its status |
| `POST /api/companies/{id}/email` | A fresh email for that company's best address. Not saved, not sent. 404 when there is no address; 422 with the reasons when the email fails its checks twice |

**Funding shown honestly.** Amounts come only from SEC Form D filings and news articles. YC companies show their batch and "Amount not disclosed"; nothing is estimated.

**Page layout** (from Dealroom, Dribbble CRM lead views and SaaS table guidance: identifying column first, secondary fields in a detail panel, filters in a toolbar above the list):

```
header: Startup Scout   "332 funded AI startups. 19 have an address to write to."
toolbar: search | stage | sort (fit, most raised, newest) | only with an email
list (420px)                        | detail
  name                    72/100    |   Company name (large)
  what they do (2 lines)            |   domain, stage, region
  $2.5M raised   Person's email     |   what they do
                                    |   funding | fit score + reason
                                    |   where we saw them (source links)
                                    |   email: to whom, found where
                                    |   [airmail-edged letter: subject + body]
                                    |   Write email / Rewrite, Copy email, Open in mail app
```

On a phone the list fills the screen and a tap opens the detail with a back link. Light and dark themes follow the system. The airmail stripe around the letter is the page's single decorative element; the letter body is set in a serif so it reads as the email itself.

**Running it:** `cd backend && cargo scout serve`, then `cd frontend && npm run dev`, then open http://localhost:3000.

**Later:** sign-in (the API then leaves localhost), payments, editing an email before it goes to Drafts, a "Send to Drafts" button that reuses `scout draft`.

## 9. Out of scope (could return later)

Telegram/WhatsApp, follow-up emails, HN, Product Hunt, ATS boards, SMTP sending. These are specified in `PLAN.v1.md`.

## 10. Docs to open before each part

- yc-oss: https://github.com/yc-oss/api
- SEC EDGAR: https://www.sec.gov/os/accessing-edgar-data · https://www.sec.gov/search-filings/edgar-application-programming-interfaces
- Tavily: https://docs.tavily.com · credits: https://docs.tavily.com/documentation/api-credits
- HN Search API: https://hn.algolia.com/api
- Anthropic structured outputs: https://platform.claude.com/docs/en/build-with-claude/structured-outputs
- async-imap: https://docs.rs/async-imap · lettre: https://docs.rs/lettre
- Gmail IMAP: https://developers.google.com/workspace/gmail/imap/imap-smtp · App Passwords: https://support.google.com/accounts/answer/185833
- texting_robots: https://docs.rs/texting_robots · scraper: https://docs.rs/scraper · psl: https://docs.rs/psl
- sqlx: https://docs.rs/sqlx · reqwest: https://docs.rs/reqwest · feed-rs: https://docs.rs/feed-rs
- GitHub Actions schedule: https://docs.github.com/en/actions/writing-workflows/choosing-when-your-workflow-runs/events-that-trigger-workflows#schedule

## 11. Compliance

- Personal one-to-one outreach, ≤10/day, real name + signature, one-line opt-out ("reply 'no' and I won't follow up").
- GDPR (EU founders): legitimate-interest basis, business addresses published for contact, keep `source_url`, honour opt-outs immediately, auto-delete contacts not emailed within 60 days.
- SEC: declared User-Agent, ≤10 req/s. Respect robots.txt.
- Never scrape logged-in LinkedIn/Wellfound. Never use GitHub data for email. Never share the dataset.

## 12. Env vars

```
DATABASE_URL=postgres://scout:scout@localhost:5433/scout
ANTHROPIC_API_KEY=
LLM_FAST_MODEL=claude-haiku-5-5
LLM_SMART_MODEL=claude-sonnet-5-5
TAVILY_API_KEY=
SEC_USER_AGENT="startup-scout/0.1 your.email@example.com"
GMAIL_USER=
GMAIL_APP_PASSWORD=
```

## 13. Cost

Sources: free · Tavily: free 1,000/month · LLM: pay-as-you-go API usage · Docker Postgres: free · Neon and GitHub Actions: free tiers.
