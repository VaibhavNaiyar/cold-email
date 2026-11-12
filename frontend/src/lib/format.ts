import type { Company, Funding, Signal } from "./types";

// Fixed zone, so the server and the browser print the same date.
const day = new Intl.DateTimeFormat("en-GB", {
  day: "numeric",
  month: "short",
  year: "numeric",
  timeZone: "UTC",
});

const UNITS: [number, string][] = [
  [1e9, "B"],
  [1e6, "M"],
  [1e3, "K"],
];

// Hand-rolled: Node and browsers disagree on Intl's compact notation, which breaks hydration.
export function money(amount: number): string {
  const [size, unit] = UNITS.find(([size]) => amount >= size) ?? [1, ""];
  const value = (amount / size).toFixed(1).replace(/\.0$/, "");
  return `$${value}${unit}`;
}

export const formatDate = (iso: string) => day.format(new Date(iso));

// The owner is in India; a fixed zone also keeps server and browser output identical.
const moment = new Intl.DateTimeFormat("en-GB", {
  day: "numeric",
  month: "short",
  year: "numeric",
  hour: "numeric",
  minute: "2-digit",
  hour12: true,
  timeZone: "Asia/Kolkata",
});

/** Date and time in IST. Form D filings carry a date only, so they get no time. */
export function formatMoment(signal: Signal): string {
  if (signal.source === "formd") return formatDate(signal.published_at);
  return `${moment.format(new Date(signal.published_at))} IST`;
}

const DAY_MS = 86_400_000;
export const OLD_AFTER_DAYS = 90;

export const daysSince = (iso: string, now: number) =>
  Math.max(0, Math.floor((now - new Date(iso).getTime()) / DAY_MS));

/** True when `days` is 0 (any date) or the date is at most that many days old. */
export const isWithin = (iso: string | null, days: number, now: number) =>
  days === 0 || (iso !== null && iso !== "" && now - new Date(iso).getTime() <= days * DAY_MS);

/** True when every word of the query appears somewhere in the given text. */
export function matchesQuery(query: string, ...parts: (string | null)[]): boolean {
  const haystack = parts.join(" ").toLowerCase();
  return query
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((word) => haystack.includes(word));
}

/** Like `age`, but in hours inside the last day, where "today" is too coarse. */
export function ago(iso: string, now: number): string {
  const hours = Math.floor((now - new Date(iso).getTime()) / 3_600_000);
  if (hours < 1) return "less than an hour ago";
  if (hours < 24) return `${hours} hour${hours === 1 ? "" : "s"} ago`;
  return age(daysSince(iso, now));
}

export function age(days: number): string {
  if (days === 0) return "today";
  if (days === 1) return "yesterday";
  if (days < 60) return `${days} days ago`;
  if (days < 730) return `${Math.round(days / 30)} months ago`;
  return `${Math.round(days / 365)} years ago`;
}

/** The news that matters most: the latest funding item, otherwise the latest item. */
export function keySignal(company: Company): Signal | null {
  return company.signals.find((signal) => signal.kind === "raised") ?? company.signals[0] ?? null;
}

export function signalLabel(signal: Signal): string {
  if (signal.source === "formd") return "Form D filed";
  if (signal.kind === "raised") return "Funding announced";
  if (signal.kind === "hiring") return "Hiring post";
  if (signal.kind === "launched") return "Launched";
  return "Seen";
}

/** Where the funding fact comes from, in words. Null when nothing is known. */
export function fundingSource(funding: Funding): string | null {
  if (funding.round) return `${funding.round} round`;
  if (funding.form_d) return "SEC Form D filing";
  if (funding.yc_batch) return `Y Combinator, ${funding.yc_batch}`;
  return null;
}

const STAGES: Record<string, string> = {
  "pre-seed": "Pre-seed",
  seed: "Seed",
  "series-a": "Series A",
  "series-b-plus": "Series B+",
  unknown: "Stage unknown",
};

export const stageLabel = (stage: string | null) => STAGES[stage ?? "unknown"] ?? stage ?? "";

const SOURCES: Record<string, string> = {
  yc: "Y Combinator",
  rss: "News",
  formd: "SEC filing",
};

export const sourceLabel = (source: string) => SOURCES[source] ?? source;

export function host(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return url;
  }
}

export type Reach = "person" | "inbox" | "none" | "unchecked";

export function reach(company: Company): Reach {
  if (company.contact) return company.contact.is_generic ? "inbox" : "person";
  return company.contacts_checked ? "none" : "unchecked";
}

export const REACH_LABEL: Record<Reach, string> = {
  person: "Person's email",
  inbox: "Team inbox",
  none: "No email found",
  unchecked: "Not searched yet",
};

const DRAFT_STATUS: Record<string, string> = {
  pending: "Waiting to reach Gmail",
  queued: "In Gmail Drafts",
  skipped: "Cancelled",
  sent: "Sent",
  replied: "Replied",
  bounced: "Bounced",
};

export const draftStatus = (status: string) => DRAFT_STATUS[status] ?? status;
