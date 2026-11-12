// Shapes returned by the Rust API (`cargo scout serve`).

export type Funding = {
  amount_usd: number | null;
  round: string | null;
  investors: string[] | null;
  yc_batch: string | null;
  form_d: boolean;
};

export type Signal = {
  title: string;
  url: string;
  source: string;
  kind: string;
  published_at: string;
};

export type Contact = {
  email: string;
  name: string | null;
  role: string | null;
  is_generic: boolean;
  source_url: string;
};

export type Draft = {
  subject: string;
  body: string;
  status: string;
};

export type Company = {
  id: number;
  name: string;
  domain: string;
  one_liner: string | null;
  stage: string | null;
  region: string | null;
  remote: boolean | null;
  score: number;
  score_reason: string | null;
  last_signal_at: string | null;
  first_seen_at: string;
  funding: Funding;
  signals: Signal[];
  contact: Contact | null;
  contacts_checked: boolean;
  draft: Draft | null;
};

export type Job = {
  id: number;
  title: string;
  /** The role on the company's own careers board. */
  url: string;
  location: string;
  remote: boolean;
  department: string | null;
  notes: string | null;
  posted_at: string | null;
  first_seen_at: string;
  /** Already listed the first time Scout read the board, so when it appeared is unknown. */
  baseline: boolean;
  fit: number | null;
  fit_reason: string | null;
  /** Judged when the role is scored; null when the company could not be placed. */
  is_startup: boolean | null;
  board: string;
  company: string;
  company_domain: string | null;
  company_about: string | null;
  company_stage: string | null;
};

export type Preview = {
  to: string;
  subject: string;
  body: string;
};
