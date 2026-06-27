"use client";
import { TraceEntry } from "@/lib/api";

const EVENT_ICON: Record<string, string> = {
  run_started: "🚀",
  plan_ready: "📋",
  task_assigned: "🎯",
  task_started: "⏳",
  task_completed: "✅",
  task_escalated: "⬆️",
  task_failed: "❌",
  run_completed: "🏁",
  run_failed: "💥",
};

const EVENT_COLOR: Record<string, string> = {
  run_started: "#60a5fa",
  plan_ready: "#a78bfa",
  task_assigned: "#94a3b8",
  task_started: "#94a3b8",
  task_completed: "#10b981",
  task_escalated: "#f59e0b",
  task_failed: "#ef4444",
  run_completed: "#10b981",
  run_failed: "#ef4444",
};

interface Props {
  trace: TraceEntry[];
}

export function TraceTimeline({ trace }: Props) {
  if (trace.length === 0) {
    return <div className="card text-[var(--muted)] text-sm">No trace events.</div>;
  }

  return (
    <div className="card flex flex-col gap-0">
      {trace.map((ev, i) => (
        <div
          key={i}
          className="flex gap-4 py-3 border-t border-[var(--card-border)] first:border-t-0"
        >
          <div className="w-6 text-center flex-shrink-0 mt-0.5">
            {EVENT_ICON[ev.event_type] ?? "•"}
          </div>
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2">
              <span
                className="text-xs font-semibold"
                style={{ color: EVENT_COLOR[ev.event_type] ?? "#94a3b8" }}
              >
                {ev.event_type}
              </span>
              <span className="text-xs text-[var(--muted)]">
                {new Date(ev.ts).toLocaleTimeString(undefined, { hour12: false, fractionalSecondDigits: 3 })}
              </span>
            </div>
            <EventDetail ev={ev} />
          </div>
        </div>
      ))}
    </div>
  );
}

function EventDetail({ ev }: { ev: TraceEntry }) {
  const d = ev.data as Record<string, unknown>;
  switch (ev.event_type) {
    case "task_assigned":
      return (
        <p className="text-sm mt-0.5">
          <span className="text-[var(--muted)]">Task</span>{" "}
          <span className="font-mono text-xs">{(d.task_id as string)?.slice(0, 8)}…</span>
          {" → "}
          <span className="font-semibold">{d.model_id as string}</span>
          {d.reason ? <span className="text-[var(--muted)] ml-1">({String(d.reason)})</span> : null}
        </p>
      );
    case "task_completed":
      return (
        <p className="text-sm mt-0.5">
          <span className="font-mono text-xs">{(d.task_id as string)?.slice(0, 8)}…</span>
          {" · "}
          <span className="font-mono">${(d.cost_usd as number)?.toFixed(5)}</span>
          {" · "}
          <span className="text-[var(--muted)]">{d.latency_ms as number}ms</span>
        </p>
      );
    case "task_escalated":
      return (
        <p className="text-sm mt-0.5">
          <span className="text-[var(--warn)]">{String(d.from_model ?? "")}</span>
          {" → "}
          <span>{String(d.to_model ?? "")}</span>
          {d.reason ? <span className="text-[var(--muted)] ml-1">({String(d.reason)})</span> : null}
        </p>
      );
    case "run_completed":
      return (
        <p className="text-sm mt-0.5">
          <span className="font-mono">${(d.total_cost_usd as number)?.toFixed(5)}</span>
          {" · "}
          <span className="font-mono">{d.total_tokens as number} tokens</span>
          {" · "}
          <span className="text-[var(--muted)]">{d.wall_ms as number}ms</span>
        </p>
      );
    default:
      return null;
  }
}
