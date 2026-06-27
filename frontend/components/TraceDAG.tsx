"use client";
import { useMemo } from "react";
import ReactFlow, {
  Background,
  Controls,
  type Node,
  type Edge,
  MarkerType,
} from "reactflow";
import "reactflow/dist/style.css";
import { TraceEntry } from "@/lib/api";

interface Props {
  trace: TraceEntry[];
  /** JSON-serialized TaskGraph: { nodes: [{id, depends_on, ...}], edges: [[from, to], ...] } */
  planGraph: unknown | null;
}

const STATUS_COLOR: Record<string, string> = {
  completed: "#10b981",
  failed: "#ef4444",
  escalated: "#f59e0b",
  running: "#60a5fa",
};

export function TraceDAG({ trace, planGraph }: Props) {
  const { nodes, edges } = useMemo(() => {
    const assigned = trace.filter(e => e.event_type === "task_assigned");
    const completed = trace.filter(e => e.event_type === "task_completed");
    const failed = trace.filter(e => e.event_type === "task_failed");
    const escalated = trace.filter(e => e.event_type === "task_escalated");

    const taskMap = new Map<string, {
      model: string;
      connection: string;
      status: string;
      cost: number;
      verify: string;
    }>();

    for (const ev of assigned) {
      const d = ev.data as Record<string, string>;
      if (!taskMap.has(d.task_id)) {
        taskMap.set(d.task_id, {
          model: d.model_id,
          connection: d.connection_id?.slice(0, 8) ?? "",
          status: "running",
          cost: 0,
          verify: "",
        });
      }
    }
    for (const ev of completed) {
      const d = ev.data as Record<string, unknown>;
      const tid = d.task_id as string;
      const info = taskMap.get(tid);
      if (info) {
        info.status = "completed";
        info.cost = (d.cost_usd as number) ?? 0;
        const vr = d.verify_result as Record<string, string> | null;
        info.verify = vr?.type ?? "passed";
      }
    }
    for (const ev of failed) {
      const d = ev.data as Record<string, string>;
      const info = taskMap.get(d.task_id);
      if (info) info.status = "failed";
    }
    for (const ev of escalated) {
      const d = ev.data as Record<string, string>;
      const info = taskMap.get(d.task_id);
      if (info) info.status = "escalated";
    }

    const taskIds = [...taskMap.keys()];
    const cols = Math.ceil(Math.sqrt(taskIds.length));

    const nodes: Node[] = taskIds.map((tid, i) => {
      const info = taskMap.get(tid)!;
      const color = STATUS_COLOR[info.status] ?? "#64748b";
      return {
        id: tid,
        type: "default",
        position: { x: (i % cols) * 220, y: Math.floor(i / cols) * 140 },
        data: {
          label: (
            <div style={{ fontSize: 11, lineHeight: 1.4 }}>
              <div style={{ fontWeight: 700, color }}>{info.model}</div>
              <div style={{ color: "#94a3b8" }}>{tid.slice(0, 8)}…</div>
              {info.cost > 0 && (
                <div style={{ color: "#64748b" }}>${info.cost.toFixed(5)}</div>
              )}
            </div>
          ),
        },
        style: {
          background: "#13131a",
          border: `1.5px solid ${color}`,
          borderRadius: 8,
          padding: "8px 12px",
          minWidth: 160,
        },
      };
    });

    // Build edges from plan graph topology if available.
    let edges: Edge[] = [];
    if (planGraph && typeof planGraph === "object") {
      const pg = planGraph as Record<string, unknown>;
      const graphNodes = pg.nodes as Array<Record<string, unknown>> | undefined;
      if (graphNodes) {
        const taskIdSet = new Set(taskIds);
        for (const n of graphNodes) {
          const tid = String(n.id ?? "");
          const deps = n.depends_on as string[] | undefined;
          if (deps && taskIdSet.has(tid)) {
            for (const dep of deps) {
              if (taskIdSet.has(dep)) {
                edges.push({
                  id: `${dep}→${tid}`,
                  source: dep,
                  target: tid,
                  type: "smoothstep",
                  style: { stroke: "#7c3aed", strokeWidth: 1.5 },
                  markerEnd: { type: MarkerType.ArrowClosed, color: "#7c3aed" },
                });
              }
            }
          }
        }
      }
    }

    return { nodes, edges };
  }, [trace, planGraph]);

  if (nodes.length === 0) {
    return (
      <div className="flex items-center justify-center h-full text-[var(--muted)] text-sm">
        No task events in trace.
      </div>
    );
  }

  return (
    <ReactFlow
      nodes={nodes}
      edges={edges}
      fitView
      fitViewOptions={{ padding: 0.2 }}
      defaultEdgeOptions={{
        markerEnd: { type: MarkerType.ArrowClosed },
        style: { stroke: "#7c3aed" },
      }}
    >
      <Background color="#1e1e2e" gap={20} />
      <Controls />
    </ReactFlow>
  );
}
