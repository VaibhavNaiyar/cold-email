# Startup Scout — Personal Job-Search Agent (build plan for Claude Code)

> Goal: every morning, find AI startups that **just raised, launched, or started hiring**, find a **publicly listed founder email with its source URL**, fill **my own email template** with a short personalized hook, and send me a digest (Telegram/WhatsApp) where I approve each email before it is sent from my Gmail.
>
> Scope: personal use, low volume (≤10 emails/day), public data + official APIs only. No LinkedIn/Wellfound scraping behind login. No guessed emails without verification.

---

## 0. Instructions for Claude Code

- Build in phases (section 9). Finish and test one phase before starting the next.
- **Before writing code against any external API, open the doc URL listed for it in section 10** and confirm endpoint shapes. Do not invent fields.
- TypeScript everywhere, Node 22+, pnpm + Turborepo. Strict TS. Zod for every external response.
- Every source adapter returns the same `RawSignal` type (section 4). Adapters must be idempotent and safe to re-run.
- Never send an email without an explicit approval record in the DB.
- Every stored email address must have a `source_url` and `source_type`. No source → don't store it.
- Respect rate limits and set a descriptive `User-Agent` with my contact email on all HTTP calls.

---

## 1. Architecture

```
            ┌──────────── daily cron (GitHub Actions / node-cron) ────────────┐
            │                                                                  │
 [Sources]  YC (yc-oss JSON) · HN Algolia · Product Hunt GraphQL · RSS funding │
            news · SEC Form D · ATS boards (Greenhouse/Lever/Ashby) · GitHub   │
            │                                                                  │
            ▼                                                                  │
 [Ingest]   RawSignal → normalize → dedupe by domain → companies table         │
            ▼                                                                  │
 [Classify] LLM (structured output): is_ai, stage, remote, hiring, region      │
            ▼                                                                  │
 [Match]    hard filters → LLM score vs my profile (0–100 + reason)            │
            ▼                                                                  │
 [Contacts] website crawl → YC founder page → GitHub commits → Hunter          │
            (domain-search w/ sources) → Hunter verifier                       │
            ▼                                                                  │
 [Draft]    my template + LLM-filled {hook} {proof} slots                      │
            ▼                                                                  │
 [Notify]   Telegram bot (default) / WhatsApp Cloud API: digest + buttons      │
            ▼  user taps SEND                                                  │
 [Send]     Gmail API drafts.create → drafts.send (or nodemailer app-password) │
            ▼                                                                  │
 [Track]    poll Gmail threads for replies → stop follow-ups                   │
            └──────────────────────────────────────────────────────────────────┘
```

## 2. Repo layout (Turborepo)

```
startup-scout/
├─ apps/
│  ├─ worker/            # cron entrypoints: ingest, enrich, draft, notify, followups
│  ├─ bot/               # Telegram webhook (or WhatsApp webhook) — approve / skip / edit
│  └─ dashboard/         # Next.js App Router: review queue, company list, sent log (optional, phase 6)
├─ packages/
│  ├─ db/                # Drizzle ORM schema + migrations (Postgres: Neon/Supabase, or SQLite for local)
│  ├─ sources/           # one file per source adapter → RawSignal[]
│  ├─ enrich/            # crawler, email extraction, Hunter client, verifier
│  ├─ llm/               # Anthropic client, prompts, Zod schemas
│  ├─ mail/              # Gmail API / nodemailer, template renderer
│  └─ config/            # env parsing (zod), profile.yaml loader
├─ profile.yaml          # my skills, target roles, locations, exclusions
├─ templates/outreach.md # MY email template with {{slots}}
├─ .github/workflows/daily.yml
└─ turbo.json
```

## 3. Config files

**profile.yaml**
```yaml
name: Vaibhav
roles: [full-stack engineer, frontend engineer, AI engineer]
stack: [TypeScript, Next.js, React, Node, WebSockets, Python/FastAPI, LLM apps]
locations: [remote-EU, remote-global, Germany]
stage: [pre-seed, seed, series-a]
must_be_ai: true
exclude_domains: []
max_emails_per_day: 10
proof_points:   # LLM picks the most relevant one
  - "Built a Turborepo monorepo with Next.js + WebSocket backend + FastAPI ML service"
  - "Built SynSights, an AI data-analytics chat interface"
  - "Shipped production sites: thelanguagesalon.com, cloudvidyaacademy.com"
```

**templates/outreach.md** — my existing template. LLM is only allowed to fill these slots:
```
Subject: {{subject}}
Hi {{first_name}},
{{hook}}            ← 1 sentence, must reference the source signal (raise / launch / job post)
<MY FIXED PARAGRAPHS>
{{proof}}           ← 1 sentence, chosen from proof_points
<MY FIXED CLOSING>
```

## 4. Database schema (Drizzle)

```ts
companies   (id, domain UNIQUE, name, one_liner, website, is_ai, stage, region, remote,
             hiring, score, score_reason, first_seen_at, last_signal_at, status)
signals     (id, company_id, source_type, source_url UNIQUE, title, published_at,
             kind ENUM('raised','launched','hiring','repo'), amount_usd NULL, raw_json)
contacts    (id, company_id, full_name, role, email, source_type, source_url,
             verification ENUM('valid','accept_all','unknown','invalid'), confidence, found_at)
drafts      (id, company_id, contact_id, subject, body, hook_signal_id, status
             ENUM('pending','approved','skipped','sent','replied','bounced'),
             gmail_draft_id, gmail_thread_id, created_at, sent_at)
followups   (id, draft_id, step INT, due_at, sent_at)
suppression (email UNIQUE, reason, created_at)   -- opt-outs, bounces, "not hiring"
runs        (id, job, started_at, finished_at, stats_json, error)
```

`RawSignal` (output of every adapter):
```ts
type RawSignal = {
  sourceType: 'yc'|'hn'|'producthunt'|'rss'|'formd'|'greenhouse'|'lever'|'ashby'|'github';
  sourceUrl: string; companyName: string; domain?: string;
  kind: 'raised'|'launched'|'hiring'|'repo'; title: string;
  publishedAt: string; amountUsd?: number; raw: unknown;
};
```

## 5. Source adapters (packages/sources)

| # | Source | How | Auth | Notes |
|---|---|---|---|---|
| 1 | **YC companies** | `GET https://yc-oss.github.io/api/companies/hiring.json` + latest `batches/{season}-{year}.json`; `meta.json` for batch names | none | Unofficial mirror of YC's Algolia index, refreshed daily. Fields: name, one_liner, website, batch, team_size, isHiring, tags, industries. Diff vs DB to detect *new*. |
| 2 | **Hacker News** | `GET https://hn.algolia.com/api/v1/search_by_date?tags=story&query=Launch%20HN&numericFilters=created_at_i>{since}`; also `tags=show_hn`; monthly "Who is hiring" thread → `/items/{id}` | none | Free; limit ~10k req/hr/IP. |
| 3 | **Product Hunt** | `POST https://api.producthunt.com/v2/api/graphql` with developer token; query today's `posts` filtered by AI topics | developer token | **Non-commercial by default** — fine for personal use only. |
| 4 | **Funding news RSS** | TechCrunch, EU-Startups, Sifted, Inc42, Entrackr, YourStory feeds (make the list configurable; verify each feed URL) → LLM extracts `{company, domain, round, amount, investors}` | none | Cheap model for extraction; dedupe by domain. |
| 5 | **SEC Form D** | EDGAR daily index / full-text search for `forms=D`; parse `primary_doc.xml` for issuer name, amount sold, related persons | none, but **User-Agent with contact email required**, ≤10 req/s | US companies only; filter out pooled funds (industry group). Gives founder names → feeds email step. |
| 6 | **ATS job boards** | Greenhouse `GET https://boards-api.greenhouse.io/v1/boards/{token}/jobs?content=true`; Lever `GET https://api.lever.co/v0/postings/{site}?mode=json` (EU: `api.eu.lever.co`); Ashby `GET https://api.ashbyhq.com/posting-api/job-board/{name}` | none | Only for companies already in DB: detect board token from their careers page HTML. Filter for engineering roles. |
| 7 | **GitHub** | REST: org repos, recent commits (`/repos/{o}/{r}/commits`) | PAT (free) | Mainly used in contact step. |

Each adapter: `fetch → zod.parse → map to RawSignal[] → upsert signals ON CONFLICT(source_url) DO NOTHING`.

## 6. Enrichment

### 6a. Classify + score (packages/llm)
- Model: `claude-haiku-5-5` for classification/extraction; `claude-sonnet-5-5` for scoring + hook writing.
- Use **Anthropic structured outputs** (`output_config.format` with a JSON schema) so responses always parse. Generate the schema from Zod.
- Classify schema: `{ is_ai: bool, stage: enum, remote_friendly: bool|null, region: string, hiring_engineers: bool|null, confidence: 0-1 }`
- Score prompt input: profile.yaml + company one_liner + latest signals + job titles. Output `{ score: 0-100, reason: string (≤25 words), best_proof_point_index: int }`.
- Hard filters before LLM: not in suppression, not contacted in last 90 days, `is_ai = true`.
- Keep top N (= max_emails_per_day × 2) for contact finding.

### 6b. Contact finding — in this order, stop at first verified founder email
1. **Company website crawl**: fetch `/`, `/about`, `/team`, `/company`, `/contact`, `/careers`, `/imprint` and `/impressum` (German companies must publish contact info there). Extract `mailto:` and email regexes; extract founder names from team page via LLM.
2. **YC company page** (`ycombinator.com/companies/{slug}`): founder names + links (names only; email usually not shown).
3. **Founder's own public pages**: personal site / GitHub profile / X bio links found in step 1–2.
4. **GitHub commits**: for the company's public org repos, read commit author emails; keep only addresses on the company domain matching a founder name; skip `noreply`.
5. **Hunter Domain Search** (`GET https://api.hunter.io/v2/domain-search?domain=…`) — returns emails *with their public sources*; prefer `type=personal`, seniority `executive`. Free tier: 50 credits/month.
6. **Verify** anything not taken verbatim from a page: Hunter Email Verifier (`/v2/email-verifier`, 0.5 credit). Accept only `valid`; mark `accept_all` as low confidence and require manual review.
7. Fallback: generic `founders@ / hello@ / jobs@` from the site, labeled as generic.

Never: guess a pattern and send unverified. Never: scrape LinkedIn.

Crawler rules: respect `robots.txt` (use `robots-parser`), 1 req/s per domain, timeout 10 s, `undici` fetch + `cheerio`; Playwright only if the page is JS-rendered and empty.

## 7. Drafting with my template (packages/mail)
- Render `templates/outreach.md`; the LLM fills **only** `subject`, `hook`, `proof` (and picks `first_name`).
- Hook must cite the specific signal (e.g., their seed round / Launch HN / new job post) — pass the signal title + URL to the prompt; reject outputs that mention facts not in the input.
- Lint before saving: ≤120 words total, no "I hope this finds you well", no emojis, plain text.
- Store as `drafts.status = 'pending'`.

## 8. Notify, approve, send, follow up

**Notify (default: Telegram, free & simplest)**
- Telegram Bot API: `sendMessage` with inline keyboard [Send] [Skip] [Edit]; webhook via `setWebhook`, handle `callback_query`.
- Digest per company: name · one-liner · signal + link · score + reason · contact + **source URL** · draft preview.

**WhatsApp (optional, like Backdoor)**
- WhatsApp Cloud API: test number can message your own verified number. Business-initiated messages outside the 24-h customer-service window require an approved **template**; simplest trick: send any message to the bot each morning to open the window, then the digest is a free-form message. Interactive reply buttons for Send/Skip.

**Send**
- Option A (simplest, personal): `nodemailer` via Gmail SMTP with an **App Password** (needs 2-Step Verification).
- Option B (proper): Gmail API — `users.drafts.create` (RFC 2822 MIME, base64url) on generate, `users.drafts.send` on approval; store `threadId`. Scope `gmail.compose` (+ `gmail.readonly` for reply detection). OAuth app in **Testing** mode with yourself as test user — note refresh tokens for Testing-mode apps expire after ~7 days; plan a re-auth command or publish as internal if you have Workspace.
- Daily cap enforced in code (`max_emails_per_day`), random 2–6 min spacing between sends.

**Follow-ups**
- Max **2** follow-ups (day +4, day +9), same thread (`threadId` + `In-Reply-To`/`References` headers), short, require approval too.
- Before each follow-up: check thread for any reply → set `replied`, cancel the rest. Bounce → add to suppression.
- Any "not interested / unsubscribe" → suppression forever.

## 9. Build phases (do them in order)

| Phase | Deliverable | Done when |
|---|---|---|
| 1 | Monorepo skeleton, env/config, Drizzle schema + migrations, `runs` logging | `pnpm db:migrate` works; `pnpm worker ingest --dry` runs |
| 2 | Adapters: YC, HN, RSS (+LLM extraction), ATS boards | `pnpm worker ingest` fills companies/signals; re-run inserts 0 duplicates |
| 3 | Classify + score with structured outputs | Top-20 list printed with reasons; unit tests on Zod schemas |
| 4 | Contact finder (crawl → GitHub → Hunter → verify) | ≥1 sourced founder email for ≥60% of top-20; every row has `source_url` |
| 5 | Template drafting + Telegram digest + approve → Gmail send | Approving in Telegram sends from my Gmail; draft without approval cannot send |
| 6 | Follow-ups, reply detection, suppression; Product Hunt + Form D adapters; Next.js dashboard | Reply stops follow-ups automatically |
| 7 | Schedule: GitHub Actions cron `30 3 * * *` (09:00 IST) or a small VPS with node-cron; alerts on failed runs | Runs unattended for 7 days |

Tests: Vitest; record real API responses as fixtures (msw) so tests don't hit the network.

## 10. Official docs / references (open before implementing each part)

**Sources**
- YC data mirror (yc-oss): https://github.com/yc-oss/api · endpoints https://yc-oss.github.io/api/meta.json
- HN Search API (Algolia): https://hn.algolia.com/api · HN Firebase API: https://github.com/HackerNews/API
- Product Hunt API v2: https://api.producthunt.com/v2/docs · schema https://github.com/producthunt/producthunt-api/blob/master/schema.graphql
- SEC EDGAR access & fair-access policy: https://www.sec.gov/os/accessing-edgar-data · EDGAR APIs: https://www.sec.gov/search-filings/edgar-application-programming-interfaces · Form D FAQ: https://www.sec.gov/resources-small-businesses/exempt-offerings/filing-form-d-notice
- Greenhouse Job Board API: https://developers.greenhouse.io/job-board.html
- Lever Postings API: https://github.com/lever/postings-api
- Ashby public job posting API: https://developers.ashbyhq.com/docs/public-job-posting-api
- GitHub REST commits: https://docs.github.com/en/rest/commits/commits

**Enrichment**
- Hunter API v2: https://hunter.io/api-documentation/v2 (domain-search, email-finder, email-verifier, email-count is free)
- robots.txt parsing: https://www.npmjs.com/package/robots-parser · cheerio: https://cheerio.js.org · Playwright: https://playwright.dev/docs/intro

**LLM**
- Anthropic structured outputs: https://platform.claude.com/docs/en/build-with-claude/structured-outputs
- Anthropic TypeScript SDK: https://github.com/anthropics/anthropic-sdk-typescript
- Prompt engineering: https://docs.claude.com/en/docs/build-with-claude/prompt-engineering/overview

**Messaging & mail**
- Gmail API drafts (create/send): https://developers.google.com/workspace/gmail/api/guides/drafts
- Gmail API sending: https://developers.google.com/workspace/gmail/api/guides/sending
- Gmail Node quickstart: https://developers.google.com/workspace/gmail/api/quickstart/nodejs
- Google OAuth (Testing-mode refresh token expiry): https://developers.google.com/identity/protocols/oauth2#expiration
- Gmail App Passwords: https://support.google.com/accounts/answer/185833 · Nodemailer: https://nodemailer.com/usage/using-gmail
- Telegram Bot API: https://core.telegram.org/bots/api
- WhatsApp Cloud API get started: https://developers.facebook.com/documentation/business-messaging/whatsapp/get-started · send messages / 24-h window: https://developers.facebook.com/documentation/business-messaging/whatsapp/messages/send-messages · templates: https://developers.facebook.com/documentation/business-messaging/whatsapp/templates/overview

**Infra**
- Turborepo: https://turbo.build/repo/docs · Drizzle ORM: https://orm.drizzle.team/docs/overview
- GitHub Actions scheduled workflows: https://docs.github.com/en/actions/writing-workflows/choosing-when-your-workflow-runs/events-that-trigger-workflows#schedule

## 11. Compliance guardrails (bake into code)
- Personal, one-to-one outreach only; ≤10/day; real name + signature; one-line opt-out ("reply 'no' and I won't follow up").
- GDPR (EU founders): legitimate-interest basis — use only business addresses published for contact, keep `source_url`, honor opt-outs immediately, auto-delete contacts not emailed within 60 days.
- Product Hunt API: non-commercial use only. SEC: declared User-Agent, ≤10 req/s. Respect robots.txt.
- Never scrape logged-in LinkedIn/Wellfound; never resell or share the dataset.

## 12. Env vars
```
DATABASE_URL=
ANTHROPIC_API_KEY=
HUNTER_API_KEY=
GITHUB_TOKEN=
PRODUCTHUNT_TOKEN=
SEC_USER_AGENT="startup-scout/0.1 your.email@example.com"
TELEGRAM_BOT_TOKEN=
TELEGRAM_CHAT_ID=
# Gmail: either
GMAIL_USER= GMAIL_APP_PASSWORD=
# or
GOOGLE_CLIENT_ID= GOOGLE_CLIENT_SECRET= GOOGLE_REFRESH_TOKEN=
# optional WhatsApp
WA_PHONE_NUMBER_ID= WA_ACCESS_TOKEN= WA_VERIFY_TOKEN= WA_MY_NUMBER=
```

## 13. Cost (personal scale)
Data sources: free · Hunter: free 50 credits/mo (Starter if you need more) · LLM: ~a few cents/day with Haiku extraction + Sonnet for top-20 · DB: Neon/Supabase free tier · Cron: GitHub Actions free · Telegram: free · WhatsApp: test number free for your own number.

## 14. First prompt to give Claude Code
> Read PLAN.md fully. Start Phase 1 only: scaffold the Turborepo exactly as in section 2, add the Drizzle schema from section 4, config loading with Zod for section 12 env vars and profile.yaml, and a `runs` logger. Then stop and show me the tree and how to run migrations. Before each later phase, open the doc links in section 10 for the APIs that phase touches.
