# NeuralForge Provider / Model / Resizable-UI Upgrade Specification

## 1. Scope

This specification defines the provider, model, and resizable-UI upgrade requirements for NeuralForge. It is intended to be implemented as a separate release-phase upgrade and is NOT part of the current final-perfection pass.

## 2. Provider Catalog Architecture

A first-class provider must have:
- Correct protocol adapter (OpenAI Responses, OpenAI Chat Completions, Anthropic Messages, Gemini Generate Content, Ollama Native, cloud/deployment providers, Custom OpenAI-Compatible, Custom Anthropic-Compatible)
- Correct configuration fields
- Secure authentication (OS keyring-backed, never exposed to renderer)
- Test Connection capability
- Discovery or deployment workflow
- Normalized models
- Meaningful errors
- Cancellation
- Tests

Incomplete integrations must be marked experimental, legacy, unavailable, or omitted — never presented as first-class.

## 3. Provider Metadata (per-provider)

Each provider definition must carry:
- ID, display name, category, protocol, base URL
- Secure credential reference (write-only to renderer)
- Organization, project, region, subscription/resource
- Deployment or endpoint
- Authentication type
- Dynamic model support flag
- Discovery mechanism
- Status (active, preview, legacy, experimental, unavailable)
- Last successful refresh timestamp
- Stale catalog state indicator
- Provider-specific configuration
- Migration metadata

## 4. Normalized Model Metadata (per-model)

Each model must carry:
- Exact ID, display name, provider, protocol
- Lifecycle status (verified, unverified, restricted, preview, deprecated, unavailable)
- Capability flags: Chat, Agent/tool, Inline, FIM/Ghost, vision, embeddings, reranking, reasoning
- Supported effort levels (none, low, medium, high, xhigh, max)
- Context limit, output limit
- Structured output support
- Streaming and cancellation support
- Local memory estimate, installed/loaded/callable state
- Account permission, region/deployment requirements
- Deprecation/replacement message
- Verification source and date

## 5. Model Discovery

- Do not assume every provider supports GET /v1/models.
- Implement provider-correct discovery for each adapter.
- Discovery does not prove callability — use capability or access probes where safe.
- On refresh failure: preserve last-known-good catalog, preserve mode assignments, preserve manual IDs, display stale-catalog state, show last successful refresh, permit retry, redact errors.
- Treat model IDs in this spec as candidate curated seeds; verify volatile IDs through official provider documentation or discovery APIs.

## 6. Settings, Provider Picker, and Model Picker

### Provider Picker
- Searchable, grouped, keyboard-accessible
- Categories, search, active/preview/legacy/experimental state
- Provider description, recently used providers
- Full keyboard navigation, screen-reader labels, responsive layout, unclipped dropdowns
- Provider forms show only relevant fields
- Actions: Test Connection, Refresh Models, Show Preview Models, Show Deprecated Models, Enter Model ID Manually, Reset Provider Defaults, Save, Cancel
- States: Connected, Unverified, Authentication Error, Unavailable, Stale Catalog, Configuration Incomplete

### Model Picker
- Searchable with provider, display name, exact ID
- Lifecycle badges, capability badges
- Context/output limits, reasoning support, local memory, installed/loaded/cloud state
- Account availability, region or deployment requirements
- Efficient handling of large dynamic catalogs

## 7. Four Mode Assignments and Effort

Independent assignments for: Chat, Agent, Inline Edit, Ghost Text.
All four configured simultaneously without hidden global overrides.
Verified: capability filtering, persistence, cancellation isolation, request correlation, provider deletion behavior, unavailable-model behavior, provider refresh behavior, duplicate model IDs across providers, no response crossover.

Effort normalization: none, low, medium, high, xhigh, max.
Each adapter may translate only supported values.
Unsupported effort values are hidden or explained — never sent universally.

## 8. Chat and Terminal Structural Resize

- One authoritative Chat width state governing Chat root, header, transcript, composer, toolbar, loading state, empty state, resize handle, drag state.
- No Chat element or invisible overlay extends into the Terminal rectangle.
- Handles: pointerdown, pointermove, pointerup, pointercancel, lostpointercapture, window blur, unmount, Chat open/close, Terminal open/close, persisted width, viewport clamping, keyboard resize, double-click reset, minimum Terminal width, minimum Chat width, maximum Chat width, cursor state, browser zoom and display scaling.
- ResizeObserver for terminal container changes.
- Prevents: fitting after disposal, observer leaks, resize loops, zero-size fitting, stale FitAddon references, unbounded animation-frame scheduling.
- Behavioral tests verify bounding rectangles, elementFromPoint, Terminal focus, typing into Terminal, Chat composer containment, overlay cleanup, pointer-capture cleanup, terminal row/column changes, restored-width clamping, 100%/125%/150%/200% scaling, Chat closed state does not intercept input.

## 9. GitHub Models Restriction

GitHub Models must NOT be added as an active provider. Migration/import guidance toward Microsoft Foundry is only supported when backed by the specification.

## 10. Volatile Model Catalog Rule

Treat model IDs as candidate curated seeds. Verify volatile IDs through official sources. When official access is unavailable: do not invent verification, do not expose as verified production default, use dynamic discovery, retain manual model ID, mark seed as unverified/restricted/preview/deprecated/unavailable, document limitation.
