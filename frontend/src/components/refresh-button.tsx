"use client";

import { useRouter } from "next/navigation";
import { useCallback, useEffect, useRef, useState } from "react";

type Status = {
  running: boolean;
  started_at: string | null;
  finished_at: string | null;
  error: string | null;
  new_roles: number | null;
  closed_roles: number | null;
};

const ENDPOINT = "/api/jobs/refresh";
const POLL_MS = 5000;

/** Reads every careers board again, then reloads the page data when the run is done. */
export function RefreshButton() {
  const router = useRouter();
  const [status, setStatus] = useState<Status | null>(null);
  const [minutes, setMinutes] = useState(0);
  const [problem, setProblem] = useState<string | null>(null);
  const wasRunning = useRef(false);

  const apply = useCallback(
    (next: Status) => {
      setStatus(next);
      if (next.started_at) {
        setMinutes(Math.floor((Date.now() - Date.parse(next.started_at)) / 60_000));
      }
      // A run that just ended has new roles in the database: fetch the page data again.
      if (wasRunning.current && !next.running) router.refresh();
      wasRunning.current = next.running;
    },
    [router],
  );

  // Results arriving after the component is gone are dropped.
  const check = useCallback(
    (isLive: () => boolean) => {
      fetchStatus()
        .then((next) => {
          if (!isLive()) return;
          apply(next);
          setProblem(null);
        })
        .catch(() => {
          if (isLive()) setProblem("Can't reach the backend.");
        });
    },
    [apply],
  );

  // Pick up a run that was already going when the page opened, then follow it while it runs.
  const running = status?.running ?? false;
  useEffect(() => {
    let live = true;
    const isLive = () => live;
    if (!running) {
      check(isLive);
      return () => {
        live = false;
      };
    }
    const timer = setInterval(() => check(isLive), POLL_MS);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [running, check]);

  const start = async () => {
    try {
      // 409 means a run is already going; its status is returned either way.
      const response = await fetch(ENDPOINT, { method: "POST" });
      if (!response.ok && response.status !== 409) throw new Error(String(response.status));
      apply(await response.json());
      setProblem(null);
    } catch {
      setProblem("Can't reach the backend. Is cargo scout serve running?");
    }
  };

  return (
    <div className="flex items-center gap-3">
      <p className="text-xs text-muted" aria-live="polite">
        {problem ?? describe(status, minutes)}
      </p>
      <button
        type="button"
        onClick={start}
        disabled={running}
        className="rounded-md border border-line px-3 py-1.5 text-sm font-semibold disabled:opacity-60"
      >
        {running ? "Refreshing…" : "Refresh jobs"}
      </button>
    </div>
  );
}

async function fetchStatus(): Promise<Status> {
  const response = await fetch(ENDPOINT, { cache: "no-store" });
  if (!response.ok) throw new Error(String(response.status));
  return response.json();
}

function describe(status: Status | null, minutes: number): string {
  if (!status) return "";
  if (status.running) {
    return `Reading every careers board and feed, about 3 minutes. ${minutes > 0 ? `${minutes} min so far.` : "Just started."}`;
  }
  if (status.error) return `Last refresh failed: ${status.error}`;
  if (status.finished_at) {
    return `Last refresh: ${status.new_roles ?? 0} new roles, ${status.closed_roles ?? 0} closed.`;
  }
  return "";
}
