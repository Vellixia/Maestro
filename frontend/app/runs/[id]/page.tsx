"use client";
import { useEffect, useState } from "react";
import { use } from "react";
import Link from "next/link";
import { api, RunSummary, TraceEntry } from "@/lib/api";
import { TraceDAG } from "@/components/TraceDAG";
import { TraceTimeline } from "@/components/TraceTimeline";

interface Props {
  params: Promise<{ id: string }>;
}

export default function RunPage({ params }: Props) {
  const { id } = use(params);
  const [run, setRun] = useState<RunSummary | null>(null);
  const [trace, setTrace] = useState<TraceEntry[]>([]);
  const [plan, setPlan] = useState<unknown>(null);
  const [loading, setLoading] = useState(true);
  const [tab, setTab] = useState<"dag" | "timeline">("dag");

  useEffect(() => {
    Promise.all([api.getRun(id), api.getRunTrace(id), api.getRunPlan(id)])
      .then(([r, t, p]) => { setRun(r); setTrace(t); setPlan(p); })
      .finally(() => setLoading(false));
  }, [id]);

  if (loading) return <div className="text-[var(--muted)] text-sm p-8">Loading…</div>;
  if (!run) return <div className="text-[var(--error)] p-8">Run not found.</div>;

  return (
    <div className="max-w-7xl mx-auto flex flex-col gap-6">
      <div className="flex items-center gap-3">
        <Link href="/runs" className="text-[var(--muted)] hover:text-[var(--foreground)] text-sm">
          ← Runs
        </Link>
        <span className="text-[var(--muted)]">/</span>
        <span className="text-sm font-mono text-[var(--muted)]">{id.slice(0, 8)}…</span>
      </div>

      <div className="card">
        <div className="flex items-start justify-between gap-4">
          <div>
            <p className="text-xs text-[var(--muted)] uppercase tracking-wider mb-1">Goal</p>
            <p className="text-lg font-medium">{run.goal}</p>
          </div>
          <span className={`badge badge-${run.status}`}>{run.status}</span>
        </div>
        <div className="flex gap-6 mt-4 text-sm">
          <div>
            <span className="text-[var(--muted)]">Cost: </span>
            <span className="font-mono">${run.total_cost_usd.toFixed(5)}</span>
          </div>
          <div>
            <span className="text-[var(--muted)]">Tokens: </span>
            <span className="font-mono">{run.total_tokens.toLocaleString()}</span>
          </div>
          {run.wall_ms != null && (
            <div>
              <span className="text-[var(--muted)]">Wall: </span>
              <span className="font-mono">{run.wall_ms}ms</span>
            </div>
          )}
          <div>
            <span className="text-[var(--muted)]">Started: </span>
            <span>{new Date(run.created_at).toLocaleString()}</span>
          </div>
        </div>
      </div>

      <div className="flex gap-2">
        {(["dag", "timeline"] as const).map(t => (
          <button
            key={t}
            onClick={() => setTab(t)}
            className={`px-4 py-2 rounded-lg text-sm font-medium transition-colors ${
              tab === t
                ? "bg-[var(--accent)] text-white"
                : "text-[var(--muted)] hover:text-[var(--foreground)]"
            }`}
          >
            {t === "dag" ? "DAG View" : "Timeline"}
          </button>
        ))}
      </div>

      {tab === "dag" && (
        <div className="card p-0 overflow-hidden" style={{ height: "480px" }}>
          <TraceDAG trace={trace} planGraph={plan} />
        </div>
      )}
      {tab === "timeline" && <TraceTimeline trace={trace} />}
    </div>
  );
}
