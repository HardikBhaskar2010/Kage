# Milestone 9 (M9) Seal Report: Autonomous Agent Runtime & Context Engine

**Milestone:** M9 — Autonomous Agent Runtime, Registry-Driven Discovery & Lineage Propagation  
**Status:** ✅ **SEALED (100%)**  
**Architecture Version:** v0.4.1  
**Target Milestone:** v1.0.0 Alpha / MVP  
**Specification Reference:** [`docs/04-ai/AI_Architecture.md`](../04-ai/AI_Architecture.md) · [`docs/02-architecture/Architecture_Contracts.md`](../02-architecture/Architecture_Contracts.md)

---

## 1. Executive Summary

Milestone 9 establishes the foundational **autonomous reasoning, planning, and execution substrate** for KAGE. Prior to M9, KAGE provided a completely governed browser engine, DevTools telemetry, and capability registry (Phases 1–8). M9 brings the agent plane online with zero architectural compromise:
- **`crates/kage-agent` Scaffolding**: Dedicated runtime crate with strict boundary separation. Zero direct CEF or CDP dependencies (`INV-01`).
- **Registry-Authoritative Tool Discovery**: The planner contains **zero hardcoded tool identifiers**. Tools are discovered dynamically via `CapabilityRegistry::query_for_agent(false)`, preserving the structural lockout of developer-only tools (`devtools.runtime.evaluate`).
- **Model Abstraction (`AgentModel`)**: Provider-neutral reasoning interface defining model metadata, structured tool calling, token accounting, and provider error mapping.
- **Unbroken Cross-Plane Lineage**: Every autonomous action generates and propagates the immutable lineage chain:
  $$\text{AgentTaskId} \longrightarrow \text{PlanStepId} \longrightarrow \text{ToolRequestId} \longrightarrow \text{ToolExecutionId} \longrightarrow \text{AuditRecordId}$$
- **Context Budgeting Engine**: Sectioned prompt pack assembly across strict budget allocations (system rules, user goals, available tools, observation, history).
- **Adversarial Prompt Boundary (`INV-03`)**: Passive web observations are enclosed within sanitized `<untrusted_web_data>` XML envelopes with explicit provenance (`trusted="false"`, `authority="none"`). Delimiter escape attempts are neutralized, and credentials are scrubbed by `SecretSanitizer` (`INV-06`).
- **Bounded Planner State Machine**: `OBSERVE -> PLAN -> VALIDATE -> EXECUTE -> RECEIVE RESULT -> UPDATE CONTEXT -> PLAN NEXT`, bound by a hard step limit (`max_steps`). Records `expected_effect` and `observed_result` for subsequent Milestone 10 verifier consumption.
- **Fail-Safe Caller STOP (`INV-09`)**: STOP cancels the agent runtime and active tool awaiters, prevents subsequent dispatches, and discards queued plan steps. It does **not** claim rollback or preemption of already-issued browser mutations or in-flight Chromium execution (eliminating the retrospective rollback illusion).

---

## 2. Crate Architecture & Boundary Separation

```text
kage-core
    ↓ (contracts, lineage, registry, policy interfaces)
kage-agent
    ├── cancellation.rs     (AgentCancellation root token)
    ├── context.rs          (ContextBudget & ContextAssembler)
    ├── execution.rs        (StepExecutor governed dispatch)
    ├── lib.rs              (Exports & contract declarations)
    ├── model.rs            (AgentModel trait & MockAgentModel)
    ├── plan.rs             (Plan, PlanStep, ActionLineage)
    ├── planner.rs          (AgentPlanner bounded state machine)
    ├── prompt_boundary.rs  (PromptBoundary & WebProvenance)
    ├── task.rs             (AgentTask, TaskState, AgentTaskId)
    └── tool_catalog.rs     (ToolCatalog dynamic registry projection)
    ↓ (planning, context, model orchestration)
src-tauri
    ↓ (actual desktop host & CEF child HWND composition)
```

### Static Invariant Proof (`INV-01`)
`cargo tree -p kage-agent` confirms zero direct dependencies on `cef`, `cef-dll-sys`, `kage-engine`, or `kage-browser`. `scripts/verify_contracts.py` includes `crates/kage-agent` in the static AST forbidden scanner.

---

## 3. Empirical Test Gate Results (11/11 Passed)

The integration test suite in `crates/kage-integration-tests/tests/phase9_agent_runtime.rs` validates all 11 gates specified in the M9 roadmap:

| Gate ID | Gate Name | Assertion & Mechanism | Typology | Status |
|---|---|---|---|---|
| **GATE-09-A** | Registry-Only Tool Discovery | `CapabilityRegistry::query_for_agent(false)` dynamically projects tools; `devtools.runtime.evaluate` is structurally absent. | Control Plane Unit | **PASS** |
| **GATE-09-B** | Cross-Plane Lineage Propagation | Verified that `AgentTaskId -> PlanStepId -> ToolRequestId -> ToolExecutionId -> AuditRecordId` is logged in `AuditDb`. | Storage & Lineage Integration | **PASS** |
| **GATE-09-C** | Model Abstraction & Proposer Semantics | Provider-neutral `AgentModel` trait emits structured `ProposedToolCall` with token usage and technical feature flags (model is an untrusted proposer, not a capability authority). | Reasoning Mock | **PASS** |
| **GATE-09-D** | Configurable Context Budgeting | `ContextBudget` envelope (default 4,000 tokens) allocates configurable bounds for system rules, goals, schemas, observations, and history. | Context Engine Unit | **PASS** |
| **GATE-09-E** | Web Authority Boundary (`INV-03`) | Web data wrapped in `<untrusted_web_data>`; delimiter breakout `</untrusted_web_data>` escaped; `kage_sec_prod_live_999` redacted. | Security Invariant Unit | **PASS** |
| **GATE-09-F** | Real Agent -> ToolBus Execution | Agent successfully proposes governed tool, which executes through `ToolBus` and records structured result. | Governance Integration | **PASS** |
| **GATE-09-G** | Caller STOP Semantics (`INV-09`) | STOP cancels runtime token, prevents subsequent dispatches, and discards queued plan steps (no rollback illusion). | Cancellation Runtime Unit | **PASS** |
| **GATE-09-H** | Multi-Step Planning Sequence | Sequential execution of multiple governed steps with continuous step indexing and status tracking. | Orchestration Integration | **PASS** |
| **GATE-09-I** | Structural Failure Handling | Mutating action without session grant triggers policy denial; agent captures `StepStatus::Denied` and pauses gracefully. | Policy Governance Unit | **PASS** |
| **GATE-09-J** | Full Physical E2E Flow | Physical AgentRuntime -> ToolBus -> real browser control-plane E2E using deterministic model substitution (tab.create + tab.list -> AuditDb hash-chain). | Physical Control Plane E2E | **PASS** |
| **GATE-09-K** | Stale / Unregistered Tool Revalidation | Revalidates proposed tool calls at execution time against live `CapabilityRegistry`; rejects unregistered tools, stale tools, developer tools, and malformed arguments. | Dynamic Governance Unit | **PASS** |

---

## 4. Component Typology: Simulation vs. Physical Browser Execution

To provide complete audit-grade transparency, M9 distinguishes clearly between:

1. **Reasoning Provider Layer (`MockAgentModel`)**:
   Used across M9 CI test gates for deterministic replay of structured tool calls without external API latency or non-deterministic completions. Real LLM adapters (OpenAI / Ollama / Anthropic) adhere to this exact same `AgentModel` trait.
   > **Note on E2E Scope:** GATE-09-J establishes a **"Physical AgentRuntime → ToolBus → real browser control-plane E2E, using deterministic model substitution."** It does not claim real provider-backed autonomous reasoning; live external LLM integration is non-deterministic and verified outside core CI regression gates.
2. **Autonomous Agent Plane (`kage-agent`)**:
   Fully physical, production runtime code executing `AgentPlanner`, `ContextAssembler`, `PromptBoundary`, `StepExecutor`, and `AgentCancellation`.
3. **Physical Browser Control Plane (`kage-browser` & `src-tauri::tools`)**:
   In `GATE-09-J`, the agent issues autonomous requests to canonical Phase 8 tools (`TabCreateTool`, `TabListTool`), executing on the live `TabManager`, creating physical tabs bound to `ProfileId("agent_sandbox")`, and mutating real tab strip state.
4. **Physical Chromium Engine (CEF 152)**:
   Physical CEF child HWND composition, GPU rendering, and multi-process architecture are empirically verified in Milestone 2 (`phase2b_integration`, `phase2d_sandbox_packaging`), Milestone 3 (`phase3_e2e`), Milestone 4 (`phase4_cdp_e2e`), and Milestone 8 (`phase8_governed_tool_suite`).

---

## 5. Empirical Physical Browser Telemetry (`GATE-09-J`)

Execution of `GATE-09-J` demonstrates the full end-to-end governed pipeline with real host browser state change:

```text
  [PASS] Gate M9-J Physical Telemetry:
    Agent Task ID:     4601d117-1ec8-46cf-9244-2aacbcf50207
    Step 0 Request ID: req_364ae51a-2d3b-494e-add7-504fe55983ad
    Step 0 Exec ID:    666e3fba-1218-457f-99da-27d6eed30407
    Physical Tab ID:   d4268e00-cbbd-441c-99c3-bfc6785b56bd
    Physical Profile:  agent_sandbox
    Audit Chain:       3 verified records in security_audit.db (SHA-256 hash-chain intact)
```

### Telemetry Pipeline Audit
1. **Agent Reasoning**: Model proposes `tab.create` with `url: "https://kage.dev/demo"` and `profile_id: "agent_sandbox"`.
2. **Registry Authority**: Dynamically validated against `CapabilityRegistry`.
3. **Policy & Audit Intent**: Dispatched via `ToolBus` declaring `ActorType::Agent`, recording initial intent into `security_audit.db` (INV-05 two-stage fail-closed lifecycle for Tier 2 mutating actions).
4. **Physical Tab Allocation**: `TabCreateTool` executes on `TabManager`, creating a live tab with UUID `d4268e00-cbbd-441c-99c3-bfc6785b56bd` partitioned under `agent_sandbox`.
5. **State Query Verification**: `tab.list` executes as Step 1, verifying that the tab exists in the active browser tab collection.
6. **Audit Finalization**: 3 audit log entries committed with immutable SHA-256 genesis hash chaining (1 `Started` intent for `tab.create` + 1 `Success` completion for `tab.create` + 1 `Success` completion for `tab.list`).

---

## 6. Execution-Time Capability & Schema Revalidation (`GATE-09-K`)

M9 explicitly distinguishes between **discovery-time projection** and **execution-time authorization**:

```text
Discovery Phase:
  CapabilityRegistry::query_for_agent(false) ──► ToolCatalog (model prompt)

Execution Phase:
  Agent proposed tool call
          ↓
  Live CapabilityRegistry lookup
          ↓
  Tool exists + not stale (fails closed if unlinked/unregistered)
          ↓
  Agent capability scope verified (developer tools prohibited)
          ↓
  Argument schema structural check (must be valid JSON object)
          ↓
  Current policy adjudication (PolicyEngine)
          ↓
  Governed ToolBus dispatch
```

`GATE-09-K` proves that even if a tool was originally present in a catalog snapshot, dynamically unregistering the tool from `CapabilityRegistry` causes immediate rejection before dispatch (`PlannerError::ToolNotFoundInCatalog`, `StepStatus::Denied`), preventing stale capability execution.

---

## 7. Model Authority Semantics

The model is strictly an **untrusted proposer**:
- **LLM**: Proposes an action: *"I would like to invoke tool X with arguments Y."*
- **Registry**: Validates presence, capability metadata, and argument schema shape.
- **Policy Engine**: Decides whether caller, actor, and session may invoke tool X under the current profile context.
- **ToolBus**: Executes the action or commits a policy denial to the audit ledger.

The model never declares itself authorized for any capability.

---

## 8. Configurable Context Budgeting

Context budgeting is governed by the configurable [`ContextBudget`](file:///c:/Users/sneha/Videos/Kage/crates/kage-agent/src/context.rs) abstraction:
```rust
pub struct ContextBudget {
    pub total_ceiling: usize,       // Default: 4,000 tokens
    pub system_budget: usize,      // Default: 500 tokens
    pub goal_budget: usize,        // Default: 300 tokens
    pub tools_budget: usize,       // Default: 1,200 tokens
    pub history_budget: usize,     // Default: 800 tokens
    pub observation_budget: usize, // Default: 1,200 tokens
}
```
All parameters are runtime-configurable via `ContextAssembler::new(budget)` while providing a stable 4,000-token default for production agent operations.

---

## 9. Established Caller-Side STOP Semantics (`INV-09`)

KAGE explicitly avoids the retrospective rollback illusion. When a user triggers STOP:

```text
User STOP
   ↓
AgentCancellation::stop()
   ├── Halts AgentPlanner reasoning loop immediately
   ├── Prevents any subsequent ToolBus dispatches
   ├── Cancels active tool awaiters via CancellationToken
   └── Discards queued plan steps
```

**Non-Rollback Boundary Contract:**
- STOP **cancels the agent runtime and active tool awaiters**, preventing future dispatches.
- STOP **does NOT claim rollback or preemption of mutations already issued to Chromium** (e.g. an HTTP request already in transit or a DOM click event already dispatched to Blink).
- Post-cancellation state reconciliation and unknown-action inspection are explicitly owned by **Milestone 10 (M10)**.
- **M10 Hardening Note**: Milestone 10 will add a concurrent race test (`PLAN STEP READY` concurrent with `STOP and DISPATCH`) to guarantee that if STOP wins the race boundary, zero requests cross into `ToolBus`.

---

## 10. Transition to Milestone 10

Milestone 9 is officially **SEALED (100% PASS)** across all 11 test gates and all architecture contracts. The next active milestone is **Milestone 10 (M10): Verified Autonomy, Postcondition Verification, STOP & Recovery**:
- Implementation of **Deterministic Postcondition Verifier (`INV-08`)** consuming `expected_effect` vs `observed_result`.
- Visual, URL, DOM state, and network silence assertion engines.
- Human Takeover Escape Hatch & graceful agent pause/resume.
- Crash recovery and uncommitted intent reconciliation.
