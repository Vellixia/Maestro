"use client";
import { useState } from "react";
import { useRouter } from "next/navigation";
import { api, TraceEntry } from "@/lib/api";
import { TraceTimeline } from "@/components/TraceTimeline";

export default function OrchestratePage() {
  const router = useRouter();
  const [goal, setGoal] = useState("");
  const [running, setRunning] = useState(false);
  const [trace, setTrace] = useState<TraceEntry[]>([]);
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [runId, setRunId] = useState<string | null>(null);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!goal.trim() || running) return;
    setRunning(true);
    setTrace([]);
    setResult(null);
    setError(null);
    setRunId(null);

    try {
      await api.orchestrateStreamFetch(
        goal,
        (ev) => {
          const d = ev.data as Record<string, unknown>;
          if (ev.event_type === "run_started") {
            setRunId(d.run_id as string ?? null);
          }
          setTrace(prev => [...prev, ev]);
        },
        (res) => {
          setResult(res || "Done.");
          setRunning(false);
        },
      );
    } catch (err: unknown) {
      setError(String(err));
      setRunning(false);
    }
  };

  return (
    <div className="max-w-4xl mx-auto flex flex-col gap-6">
      <h1 className="text-2xl font-bold">Orchestrate</h1>

      <form onSubmit={submit} className="card flex flex-col gap-3">
        <label className="text-xs font-semibold text-[var(--muted)] uppercase tracking-wider">
          Goal
        </label>
        <textarea
          value={goal}
          onChange={e => setGoal(e.target.value)}
          rows={3}
          placeholder="Research the top 5 LLM providers and compare their pricing, then write a concise summary report…"
          className="w-full bg-[var(--background)] border border-[var(--card-border)] rounded-lg px-4 py-3 text-sm resize-none focus:outline-none focus:border-[var(--accent)] transition-colors"
          disabled={running}
        />
        <div className="flex items-center gap-3">
          <button
            type="submit"
            disabled={running || !goal.trim()}
            className="px-5 py-2 rounded-lg bg-[var(--accent)] text-white text-sm font-medium hover:bg-[var(--accent-light)] hover:text-black transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {running ? "Running…" : "Run"}
          </button>
          {runId && (
            <button
              type="button"
              onClick={() => router.push(`/runs/${runId}`)}
              className="text-sm text-[var(--accent-light)] hover:underline"
            >
              View full trace →
            </button>
          )}
        </div>
      </form>

      {error && (
        <div className="card border-[var(--error)] text-[var(--error)] text-sm">
          Error: {error}
        </div>
      )}

      {result && (
        <div className="card">
          <p className="text-xs font-semibold text-[var(--muted)] uppercase tracking-wider mb-2">
            Result
          </p>
          <pre className="text-sm whitespace-pre-wrap">{result}</pre>
        </div>
      )}

      {trace.length > 0 && (
        <div>
          <p className="text-xs font-semibold text-[var(--muted)] uppercase tracking-wider mb-2">
            Live Trace
          </p>
          <TraceTimeline trace={trace} />
        </div>
      )}
    </div>
  );
}
