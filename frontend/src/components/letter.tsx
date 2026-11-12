"use client";

import { useState } from "react";
import { draftStatus, host } from "@/lib/format";
import type { Company, Preview } from "@/lib/types";

type State =
  | { kind: "idle" }
  | { kind: "writing" }
  | { kind: "ready"; preview: Preview }
  | { kind: "failed"; message: string; problems: string[] };

// Previews live for the session, so switching companies does not pay for a rewrite.
const written = new Map<number, Preview>();

export function Letter({ company }: { company: Company }) {
  const saved = written.get(company.id);
  const [state, setState] = useState<State>(saved ? { kind: "ready", preview: saved } : { kind: "idle" });
  const [copied, setCopied] = useState(false);
  const { contact, draft } = company;

  if (!contact) {
    return (
      <p className="mt-3 rounded-lg border border-dashed border-line p-5 text-sm leading-6 text-muted">
        {company.contacts_checked
          ? "No published address was found for this company, so there is no one to write to yet."
          : "This company hasn't been searched for an address yet. The contacts job reaches it on a later run."}
      </p>
    );
  }

  const write = async () => {
    setState({ kind: "writing" });
    try {
      const response = await fetch(`/api/companies/${company.id}/email`, { method: "POST" });
      const body = await response.json();
      if (!response.ok) {
        setState({ kind: "failed", message: body.error ?? "The email could not be written.", problems: body.problems ?? [] });
        return;
      }
      written.set(company.id, body);
      setState({ kind: "ready", preview: body });
    } catch {
      setState({ kind: "failed", message: "The backend didn't answer. Check that `cargo scout serve` is running.", problems: [] });
    }
  };

  const email = draft
    ? { to: contact.email, subject: draft.subject, body: draft.body }
    : state.kind === "ready"
      ? state.preview
      : null;

  const copy = async () => {
    if (!email) return;
    await navigator.clipboard.writeText(`Subject: ${email.subject}\n\n${email.body}`);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="mt-3">
      <p className="text-sm text-muted">
        To {contact.name ? `${contact.name}${contact.role ? `, ${contact.role}` : ""}` : "the team inbox"}
        {" at "}
        <span className="text-ink">{contact.email}</span>. Found on{" "}
        <a href={contact.source_url} target="_blank" rel="noreferrer" className="text-navy underline-offset-4 hover:underline">
          {host(contact.source_url)}
        </a>
        .
      </p>

      <div className="airmail mt-4">
        <div className="rounded-[5px] bg-surface p-5 sm:p-8">
          {email ? (
            <>
              <p className="border-b border-line pb-3 font-semibold">{email.subject}</p>
              <div className="mt-4 whitespace-pre-wrap font-serif text-[1.05rem] leading-7">{email.body}</div>
            </>
          ) : state.kind === "writing" ? (
            <p className="py-8 text-center text-muted" aria-live="polite">
              Writing a personal email for {company.name}. This takes a few seconds.
            </p>
          ) : (
            <div className="py-6 text-center">
              <p className="mx-auto max-w-[46ch] text-sm leading-6 text-muted">
                See the email {company.name} would get, written from your template and their funding news. Nothing is saved or sent.
              </p>
              <button type="button" onClick={write} className="mt-4 rounded-md bg-navy px-4 py-2 text-sm font-semibold text-surface">
                Write email
              </button>
            </div>
          )}
        </div>
      </div>

      {state.kind === "failed" && (
        <div className="mt-3 text-sm text-red" role="alert">
          <p>{state.message}</p>
          {state.problems.length > 0 && (
            <ul className="mt-1 list-disc pl-5">
              {state.problems.map((problem) => (
                <li key={problem}>{problem}</li>
              ))}
            </ul>
          )}
        </div>
      )}

      {email && (
        <div className="mt-4 flex flex-wrap items-center gap-3 text-sm">
          {draft ? (
            <span className="font-semibold">{draftStatus(draft.status)}</span>
          ) : (
            <button type="button" onClick={write} disabled={state.kind === "writing"} className="rounded-md border border-line px-3 py-2 disabled:opacity-50">
              {state.kind === "writing" ? "Rewriting…" : "Rewrite"}
            </button>
          )}
          <button type="button" onClick={copy} className="rounded-md border border-line px-3 py-2">
            {copied ? "Copied" : "Copy email"}
          </button>
          <a
            href={`mailto:${email.to}?subject=${encodeURIComponent(email.subject)}&body=${encodeURIComponent(email.body)}`}
            className="rounded-md border border-line px-3 py-2"
          >
            Open in mail app
          </a>
        </div>
      )}
    </div>
  );
}
