# Maestro — Capability-Aware LLM Orchestration Platform (Rust)

> Register many models across many providers (incl. free). Measure each model's capability once on insertion (and keep it fresh). Then **plan** a goal, **decompose** it into a task DAG, and **assign each piece to the cheapest model that is good enough** — weak/free models do easy work, strong models do hard work — with **verify-and-escalate** to protect quality. Drop-in `auto` model + native `/v1/orchestrate`.

---

## Context — why this is being built

The predecessor project was a mature **AI gateway** (JS/Next.js): 100+ provider integrations, OpenAI/Claude/Gemini-compatible API, a per-model **capability matrix**, **pricing**, **combos** (fallback/round-robin/fusion), and **account fallback + rate-limit cooldown**. But it routes **one request → one model**. It has *no* planning, no task decomposition, no difficulty→capability matching, and no quality calibration. We are rebuilding around **orchestration**, in a new stack.

**Decisions locked with the user:**
1. **Entire engine in Rust.** (Frontend in Next.js.)
2. **Storage = SurrealDB**, single multi-model store (graph + vector + relational + ACID), embedded.
3. **Provider gateway = pure Rust, curated + OpenAI-compat tail** — native OpenAI/Anthropic/Gemini via the `genai` crate; the long tail of providers as OpenAI-compatible config rows; bespoke reverse-engineered providers (Cursor protobuf, etc.) deferred.
4. **Unit of work = one-shot task decomposition** (goal → plan → assign → run DAG → verify/escalate → synthesize → answer). No agent tool-loops in v1, but interfaces designed to allow them later.
5. **Hybrid calibration** — public benchmark/Elo **priors** + a small auto-graded **probe suite**, fused into a capability profile **per provider-connection**, refreshed over time.

**Intended outcome:** right-size every piece of work, cutting cost 50–90% on mixed-difficulty jobs while holding a quality floor via verify-and-escalate, and exploiting free tiers safely. Full per-subtask trace for trust.

---

## Tech stack (concrete)

| Concern | Choice | Why |
|---|---|---|
| Language (engine) | **Rust** (Cargo workspace, edition 2021) | User requirement; perf + safety for the profile/DAG logic |
| Async runtime | **tokio** | Standard; powers everything below |
| HTTP server / API | **axum** (+ tower, hyper) | Ergonomic, tokio-native; `axum::Sse` for streaming out |
| Provider clients | **`genai`** as the multi-provider base | Unifies OpenAI/Anthropic/Gemini; **custom-header support** (needed for OAuth/anti-ban). `rig` considered; `genai` wins on header control |
| OpenAI server schema | **`async-openai-types`** | Reuse exact OpenAI request/response structs so tools (Claude Code/Cursor) point at us unchanged |
| Inbound SSE | **`eventsource-stream`** + reqwest | Parse provider streams |
| Format translation | **hand-rolled** (no crate exists) | OpenAI = canonical internal schema; translate ↔ Anthropic/Gemini incl. streaming chunks, tool-call ID sanitizing, thinking/reasoning normalization. Track **LiteLLM→Rust** port (BerriAI, ~Sep 2026) to fold in later |
| Storage | **SurrealDB** embedded (`surrealdb` crate, `kv-rocksdb` in prod; `Surreal<Any>` so tests use in-mem, scale can go remote) | Single store for graph + vector(HNSW/DiskANN) + relational + ACID. Caveat: pure-Rust SurrealKV engine is beta → run **RocksDB on-disk** in prod |
| DAG structure | **`petgraph`** + tokio `JoinSet` + per-provider `Semaphore` | In-process topological scheduling, bounded parallelism. External queue only if many concurrent runs |
| Embeddings | **`fastembed`** (local, e.g. BGE-small) with provider-embedding fallback | Plan/result cache + capability similarity + dedup without per-embedding cost; offline-capable |
| Code verification sandbox | **Docker ephemeral containers** (`bollard`) for MVP; `wasmtime` for untrusted later | Run tests/lint to verify code subtasks |
| Auth | **`jsonwebtoken`** + API keys, **`argon2`** for password hash | Mirror 9router's model |
| Observability | **`tracing`** + structured per-run trace persisted in SurrealDB | Drives the Trace UI |
| Frontend | **Next.js** (App Router) + React + **React Flow** (DAG trace) + **Recharts** (cost/usage) + Tailwind | Talks to Rust API over REST + SSE; reuse 9router dashboard UX patterns |

**Why SurrealDB over the user's initial pick HelixDB:** HelixDB is Rust + Apache-2.0 and strong at vector+graph, but it is **not embeddable** (separate server/container), its OSS local default is **in-memory and wipes on stop**, it uses a **compiled bespoke query language** (HelixQL — LLMs can't generate it), and it is **weak for relational/usage-log data** (single-writer ceiling) — so you'd run it *plus* Postgres anyway, defeating the single-binary goal. SurrealDB delivers the embeddable, multi-model, single-store story HelixDB cannot today. (Fallback if SurrealDB proves immature in load testing: **Postgres + pgvector** as system-of-record, graph via adjacency/recursive CTEs.)

---

## The core correction (what the original idea missed)

The naive framing — "predict task difficulty, send dumb model to easy / smart to hard" — breaks in practice. Five corrections:

1. **Capability is a vector, not a scalar.** A model can be great at code, weak at math, strong at long-context, unreliable at JSON. Match **per dimension**.
2. **Difficulty prediction is unreliable** → pure upfront assignment mis-routes. Fix: a **cascade** — assign the cheapest capable model, **verify** the output, **escalate** to a stronger model on failure. Robust to bad predictions (FrugalGPT pattern).
3. **One-time calibration drifts** (providers swap weights, free tiers get quantized/throttled). Calibrate once but **refresh**: periodic light re-probes + **online learning** — every production subtask is a free labeled datapoint (did model M pass verification on a task needing skills S?).
4. **Same model id ≠ same quality across providers.** Calibrate **per provider-connection**, not per model id.
5. **Hard constraints ≠ soft preferences.** Context window, modality (vision/audio), tool-calling, strict-JSON support are **hard filters** applied *before* cost optimization — never traded against price.

Plus operational realities the original framing skipped: the planner/classifier/verifier **cost money** (meta-cost → cheap classifiers, caching, a **trivial fast-path**, stakes-scaled verification); subtasks form a **DAG** (parallelism is a feature); **verification differs by output type**; and **policy/privacy** must be able to forbid routing sensitive data to free/public endpoints.

---

## Architecture (subsystems)

```
                 ┌─────────────────────────────────────────────────────────┐
 goal ─▶ API ───▶│ ORCHESTRATION CORE (Rust crates)                         │─▶ answer + trace
 (model="auto"   │  Planner → Classifier → Assignment Engine → DAG Executor │
  or /orchestrate)│     ▲          │              │            │             │
                 │  plan cache  RequirementProfile │       Verifier+Escalate │
                 │                              ▼  ▼            │            │
                 │                          Synthesizer ◀───────┘            │
                 └──────┬───────────────────────────────────────┬──────────┘
                        │ reads capability profiles              │ calls models
            ┌───────────▼─────────────┐            ┌─────────────▼───────────┐
            │ Capability Registry +   │            │ Provider Gateway (Rust) │
            │ Calibration Engine      │            │ genai + axum + transl.  │
            │ priors + probes →       │            │ native OAI/Claude/Gemini│
            │ profile per connection  │            │ + OpenAI-compat tail    │
            └───────────┬─────────────┘            │ + account fallback/rl   │
                        │                          └─────────────┬───────────┘
                ┌───────▼──────────────────────────────────────▼─────────┐
                │ SurrealDB (graph + vector + relational, ACID, embedded) │
                │ + Trace UI (Next.js / React Flow)                       │
                └─────────────────────────────────────────────────────────┘
        (Online Learning Loop feeds calibration; Policy/Budget Engine gates the router)
```

### 1. Provider Gateway — `crates/gateway` (Rust)
Call any model on any provider through one interface. Built on `genai` + a **hand-rolled translation core** with **OpenAI Chat Completions as the canonical internal schema**.
- Native: OpenAI, Anthropic, Gemini. Long tail: **OpenAI-compatible config rows** (base URL + headers + model map) — collapses most of "100+ providers" into data, not code.
- Port *concepts* (not code) from 9router: **account fallback + rate-limit cooldown**, **OAuth token refresh**, multi-account rotation, format translation (tool-call ID sanitizing, thinking normalization, streaming-chunk transform).
- Deferred: bespoke reverse-engineered transports (Cursor protobuf, etc.) as opt-in executors.
- Exposes both an internal Rust API (for the orchestrator) and the public OpenAI-compatible HTTP surface.

### 2. Capability Registry & Calibration Engine — `crates/registry`, `crates/calibration` (the heart)
On model insertion, build a **CapabilityProfile** *per connection*:
- **Skill vector (0–100 + confidence)**: `reasoning, coding, math, instructionFollowing, longContextRecall, toolCalling, structuredOutput(JSON), factuality, multilingual, writing`.
- **Hard constraints**: `contextWindow, maxOutput, vision, audio, supportsTools, supportsJsonMode`.
- **Operational**: `costInPerM, costOutPerM, latencyTokPerSec, errorRate, freeTierLimits`.
- **Hybrid measurement:**
  - *Priors* — seed each dimension from a `benchmark_priors` table keyed by model family/id (LMArena Elo, Artificial Analysis index, MMLU/GPQA/HumanEval/SWE-bench/IFEval). Instant coverage, no cold start.
  - *Probes* — compact **calibration suite**, ~8–15 auto-gradable items **per dimension**. **Graders**: exact/regex, **code → run unit tests in Docker sandbox**, **JSON → schema validation**, math → numeric tolerance / self-consistency, open-ended → **LLM-judge using a trusted anchor model**.
  - *Fusion* — combine prior + observed into a posterior (weight observed more as N grows); emit score + confidence.
- **Refresh**: background re-probe cron + **online updates** from production verification outcomes. Free/rate-limited models onboard **priors-first**, probe in background.

### 3. Planner — `crates/planner`
Goal (+ context/constraints/budget) → **TaskGraph (DAG)**. Each node: `{id, instruction, inputs[], outputType, requiredSkillsHint, stakes, verifySpec}`. Planning is routed to a high-reasoning model. **Trivial fast-path**: simple goals skip planning → single task. **Plan cache** keyed by goal embedding (SurrealDB vector search); optional **recipe templates** (research / code-feature / bulk-extraction).

### 4. Task Classifier / Difficulty Estimator — `crates/classifier`
Per subtask → **RequirementProfile**: required level per skill dimension, hard constraints (vision? tools? min context? strict JSON?), stakes. Cheap by design: heuristics + embeddings + a small classifier model. **Never** an expensive model here.

### 5. Assignment Engine / Router — `crates/router` (core optimization)
Given RequirementProfile + pool of CapabilityProfiles + live availability + policy:
1. **Hard filter** — drop models violating any hard constraint (ctx too small, no tools/vision/JSON, rate-limited/exhausted, privacy-disallowed).
2. **Capability dominance + margin** — keep models whose capability ≥ requirement + safety margin on every *required* dimension.
3. **Objective** — minimize cost (free → cheap → premium), optionally trading latency. *This is "dumb does easy, smart does hard," done correctly.*
4. Attach an **escalation ladder** (ordered stronger-capable fallbacks).
MVP: greedy per-node + a **global budget guard**. Later: ILP across the whole DAG under a global budget.

### 6. DAG Executor — `crates/executor`
`petgraph` topological schedule; independent nodes run **in parallel** (tokio `JoinSet`), bounded by per-provider `Semaphore` (respect free-tier rate limits). Assemble only the needed context per node (context economy). Retries/timeouts. Emits a live per-node trace.

### 7. Verifier / Quality Gate + Escalation — `crates/verifier` (reliability backbone)
Per output type: code → tests/lint/compile (Docker sandbox); JSON → schema; math/number → recompute / self-consistency; classification → confidence / self-consistency; prose → **LLM-judge vs rubric** or self-critique. On fail → **escalate** up the ladder (cap attempts); optionally re-plan a mis-scoped node. **Strictness scales with stakes**. High-stakes nodes can route to human review / abort.

### 8. Synthesizer / Reducer — `crates/synthesizer`
Composes subtask outputs into the final deliverable (capability-matched; sometimes the terminal DAG node *is* the synthesis). Streams output + progress events.

### 9. Policy & Budget Engine — `crates/policy`
Per-request controls: `maxCost`, `maxLatency`, `qualityFloor`, `privacy` (e.g. "never send to free/public providers", data residency), allow/block provider lists. Modes: *cheapest-viable* (default), *fastest*, *highest-quality-within-$X*, *free-only*.

### 10. API — `crates/api`
- **OpenAI-compatible drop-in**: model name `auto` triggers the full pipeline (existing tools point at us unchanged).
- **Native** `/v1/orchestrate`: returns final result + full trace; SSE streaming + subtask progress events.
- Admin/REST for the dashboard (models, connections, calibration, runs, usage).

### 11. Shared types — `crates/core-types`
`CapabilityProfile`, `RequirementProfile`, `TaskGraph`/`TaskNode`, `RunTrace`, `Policy` — serde-(de)serializable, shared across crates.

---

## Data model (SurrealDB)

Relational/document tables: `model`, `connection` (credentials, priority, availability/cooldown), `capability_profile` (skill vector + constraints + ops + confidence + calibratedAt), `benchmark_prior`, `calibration_run`, `run`, `subtask`, `subtask_attempt` (model, tokens, cost, latency, verifyResult, escalatedFrom), `usage`, `settings`, `api_key`.
**Graph edges**: `connection->serves->model`, `model->from->provider`, `subtask->depends_on->subtask` (the DAG), `attempt->escalated_to->attempt`.
**Vector indexes (HNSW/DiskANN)**: on `capability_profile.embedding`, `plan_cache.goal_embedding`, `result_cache.input_embedding` — all queried with SurrealQL vector search alongside relational filters in one engine.

---

## Two key flows

**A. Model onboarding / calibration:** `add model+connection` → fetch **priors** by family/id → run **probe suite** (auto-graded; anchor-judge for open-ended) → **fuse** prior+observed → store **CapabilityProfile per connection** → mark routable. Background: periodic re-probe + online updates from production verification.

**B. Request orchestration:** `goal` → *trivial?* → **single-model fast-path** : else **Planner** → **TaskGraph (DAG)** → per node: **Classify** → **Assign** (hard filter → dominance+margin → cheapest) → **Execute** → **Verify** → *fail?* → **Escalate** → pass outputs along edges → **Synthesize** → return **result + trace** → emit **learning signals**.

---

## Use cases
1. **Coding feature** — strong model writes the tricky core; free/cheap models write boilerplate, tests, docs **in parallel**; verifier runs the tests; failures escalate.
2. **Research/report** — strong model frames questions → many **free** models extract/summarize sources in parallel → strong model synthesizes.
3. **Bulk extraction/classification** — free model handles the easy 95%; **low-confidence items escalate** to a strong model. Massive saving at scale.
4. **Content pipeline** — outline (strong) → sections (cheap, parallel) → consistency/polish (mid).
5. **Budget-capped task** — "do X for ≤ $0.02" → maximize free/cheap within budget; escalate only if verification fails and budget allows.
6. **Drop-in cost optimizer** — point any OpenAI-compatible tool at `auto`; every request is right-sized automatically.

## Real problems solved
**Cost** (stop overpaying premium for easy work; safely exploit free tiers) · **Quality** (verify-and-escalate guarantees a floor) · **Speed** (parallel free models beat serial premium) · **Reliability** (rate-limit-aware scheduling + multi-account fallback) · **Trust** (full per-subtask trace of who/why/how-much).

---

## Repo layout
```
maestro/                          (Cargo workspace)
  crates/
    core-types/   shared structs (profiles, TaskGraph, trace, policy)
    gateway/      genai + translation core + account fallback + OAuth + OpenAI-compat tail
    registry/     model + connection registry, capability profiles
    calibration/  priors loader, probe suite, graders (sandbox+judge), fusion, recalibration
    planner/      decomposition, plan cache, recipe templates
    classifier/   requirement-profile estimator
    router/       hard filter + dominance match + cost objective + escalation ladder
    executor/     petgraph DAG scheduler (parallel), context assembly
    verifier/     per-type verification + escalation control
    synthesizer/  reducer
    policy/       budget/latency/privacy modes
    storage/      SurrealDB access layer (repos, migrations, vector indexes)
    api/          axum server: /v1/chat/completions(auto), /v1/orchestrate, admin REST, SSE
  frontend/       Next.js app: onboarding/calibration UI, run Trace UI (React Flow), usage (Recharts)
```

## Build phases (each delivers value)
- **Phase 0 — Gateway + storage spine.** `gateway` (OpenAI/Anthropic/Gemini native + OpenAI-compat config rows + streaming + account fallback) + `storage` (SurrealDB) + OpenAI-compatible passthrough via `api`. Outcome: "call any model uniformly, in Rust." *(Largest single effort — research pegs a focused gateway at ~4–7 months for one strong Rust engineer; the long tail is ongoing maintenance.)*
- **Phase 1 — Capability registry + hybrid calibration.** `benchmark_priors`, probe suite + graders (Docker sandbox + anchor-judge), profile fusion, per-connection profiles, onboarding flow + calibration UI.
- **Phase 2 — Single-task router (no planning yet).** `classifier` + `router` (hard filter → dominance → cheapest) + `verifier` + escalation. Ship as the `auto` model. **Already a useful drop-in cost optimizer.**
- **Phase 3 — Planner + DAG executor + synthesizer.** Full decomposition orchestration via `/v1/orchestrate`.
- **Phase 4 — Learning loop + Trace UI + policy/budget modes + caching.**
- **Phase 5 (future).** Agentic tools, global ILP optimizer, plan-template library, bespoke reverse-engineered providers.

## Risks & mitigations
- **Gateway effort is the critical path.** Mitigate: canonical OpenAI schema + OpenAI-compat endpoints (incl. Gemini's/Anthropic's compat layers) for the common feature set; defer bespoke providers; fold in the LiteLLM→Rust transform library when it ships. *Anthropic's OpenAI-compat layer drops caching/structured-output/thinking — OAuth/subscription providers still need native paths.*
- **SurrealDB maturity.** Run **RocksDB on-disk** in prod (not beta SurrealKV); keep vector indexes RAM-sized or use DiskANN; load-test recall/throughput on 3.1+ before committing. Fallback: Postgres + pgvector.
- **Calibration cost/accuracy.** Small probe suites + priors-first onboarding for rate-limited free models; online learning to converge over time.
- **Meta-cost of orchestration.** Trivial fast-path, plan/result caching, cheap classifier, stakes-scaled verification.
- **Confident-but-wrong cheap outputs.** Scale verification strictness to stakes; human-review/abort path for high stakes.

## Verification (how to test end-to-end)
- **Calibration sanity**: onboard a known-weak free model and a known-strong model; assert profiles rank them correctly per dimension and probes move scores off priors.
- **Per-connection degradation**: onboard the *same* model id on two providers (one quantized/free); assert distinct profiles.
- **Routing**: an obviously-easy task routes to a **free** model; an obviously-hard task routes to a **strong** model (assert via trace).
- **Cascade**: feed an *easy-looking-but-hard* task; assert the cheap model is tried, verification fails, and it **escalates** (visible in trace).
- **Budget**: set `maxCost`; assert the run respects it and prefers free/cheap.
- **Parallelism**: a DAG with independent nodes runs them concurrently (wall-clock < sum of node times).
- **Unit tests** (`cargo test`): dominance/margin matcher, classifier outputs, each grader (code/JSON/math/judge), petgraph topological scheduling, escalation ladder.
- **Gateway conformance**: golden-file tests for OpenAI↔Anthropic↔Gemini translation incl. streaming chunks + tool calls; point real Claude Code/Cursor at `auto` as a smoke test.
- **Integration**: run all 6 use-case scenarios; compare total cost & quality vs an all-premium baseline (expect major cost drop at verified-equal quality).
