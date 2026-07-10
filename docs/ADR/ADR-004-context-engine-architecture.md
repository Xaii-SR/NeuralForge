# ADR-004: Context Engine Architecture

**Status:** Planned  
**Date:** 2026-07-10  
**Author:** Cline (AI Agent)  

## 1. Current Repository Understanding Capabilities

### Existing Indexer (`database::indexer`)

- **Chunking:** 40-line windows with 5-line overlap
- **Filtering:** Skips non-text files via null-byte detection (`is_probably_text`)
- **Caching:** Skips unchanged files via content hash comparison
- **Storage:** FTS5 full-text search index in per-workspace `index.db`
- **Trigger:** Manual via `index_workspace()` command (called from `lib/ai.ts` `indexWorkspace()`)

### Existing Search System (`database::search`)

- **Query conversion:** Natural language converted to OR-of-terms FTS5 query
- **Stemming:** Porter stemmer enabled via FTS5 configuration
- **Ranking:** FTS5 built-in BM25 ranking
- **Returns:** Path, line numbers, content snippet, score

### Existing Resolver System (`database::resolver`)

- **Purpose:** Maps natural-language file references to actual workspace paths
- **Strategy:** Filename match outranks content-only match
- **Ambiguity:** Returns candidates when multiple files match (human disambiguation)
- **Output:** `FileCandidate { path, score, match_kind }` or resolved path

### Database Storage

- **Engine:** SQLite with FTS5 (bundled via `rusqlite` `bundled-full` feature)
- **Schema:** `chunks` table with columns: `id, file_path, start_line, end_line, content, content_hash, embedding BLOB`
- **Note:** The `embedding BLOB` column exists in schema but is never populated — no embedding model integration has been implemented
- **Persistence:** One `index.db` per opened workspace

### Symbol Handling

- **Current:** None. The indexer treats all source files as plain text with no AST or symbol awareness.
- **Imports/relationships:** No extraction or tracking exists.
- **Dependencies:** No analysis of file-to-file relationships.

## 2. Current Limitations

| Capability | Status | Impact |
|---|---|---|
| Semantic embeddings pipeline | ❌ Not implemented | FTS5 keyword search misses semantically related concepts |
| Dependency graph understanding | ❌ Not implemented | Cannot determine which files are affected by a change |
| Automatic relevant file selection | ❌ Not implemented | Users must manually specify file paths |
| Context ranking system | ❌ Not implemented | No mechanism to prioritize most relevant context within token budgets |
| Conversation-aware retrieval | ❌ Not implemented | Each query is independent; no multi-turn context refinement |
| Symbol-level indexing | ❌ Not implemented | No knowledge of functions, structs, classes, or imports |
| Cross-file relationship tracking | ❌ Not implemented | Cannot trace symbol usage across module boundaries |
| Token budget management | ❌ Not implemented | Context injection has no size guard — oversized prompts possible |
| End-to-end chat-time context injection | ⚠️ Partial | `get_context_for_query` exists but is not proven as an automatic chat-time feature |
| Vector/semantic search | ❌ Schema-ready only | `chunks.embedding BLOB` column exists, never populated |

## 3. Proposed Context Engine Architecture

```
User Request
     │
     ▼
┌──────────────────┐
│  Intent Analyzer  │
│  (classify        │
│   request type)   │
└──────┬───────────┘
       │
       ▼
┌─────────────────────────────┐
│  Repository Understanding   │
│  Layer                      │
│  ┌───────────────────────┐  │
│  │ Repository Scanner    │  │
│  │ Symbol Index          │  │
│  │ Dependency Graph      │  │
│  └───────────────────────┘  │
└──────┬──────────────────────┘
       │
       ▼
┌─────────────────────────────┐
│  Context Retrieval Engine   │
│  ┌───────────────────────┐  │
│  │ FTS5 Keyword Search   │  │
│  │ Semantic Search       │  │
│  │ Symbol Lookup         │  │
│  │ Dependency Traversal  │  │
│  └───────────────────────┘  │
└──────┬──────────────────────┘
       │
       ▼
┌─────────────────────────────┐
│  Ranking System             │
│  ┌───────────────────────┐  │
│  │ Context Ranker        │  │
│  │ Context Budget        │  │
│  │ Manager               │  │
│  └───────────────────────┘  │
└──────┬──────────────────────┘
       │
       ▼
┌─────────────────────────────┐
│  Relevant Context Package   │
│  (structured prompt input)  │
└──────┬──────────────────────┘
       │
       ▼
┌─────────────────────────────┐
│  AI Model                   │
│  (routed by Model Router)   │
└─────────────────────────────┘
```

## 4. Component Design

### 4.1 Repository Scanner

**Responsibilities:**
- File discovery and enumeration across the workspace
- Language detection (file extension → programming language)
- Metadata collection (file size, modification time, line count)
- Ignore-pattern respect (`.gitignore`, `.neuralforge-ignore`)

**Input:** Workspace root path  
**Output:** `Vec<ScannedFile { path, language, size, modified_at, lines }>`

**Design notes:**
- Reuses existing `walkdir` dependency already in `Cargo.toml`
- Language detection via extension mapping (`.rs` → Rust, `.ts` → TypeScript, etc.)
- Skips binary files, node_modules, target/, .git (compatible with current convention)

### 4.2 Symbol Index

**Responsibilities:**
- Extract function definitions, struct declarations, class definitions, imports
- Track symbol names, file locations (line ranges), and visibility
- Enable "go to definition" and "find references" queries
- Store in SQLite for queryability

**Input:** Scanned files from Repository Scanner  
**Output:** `Vec<Symbol { name, kind, file_path, start_line, end_line, visibility }>`

**Design notes:**
- Requires per-language parsing — start with RegExp-based extraction for Rust and TypeScript (fast, no heavy parsing deps)
- Future: tree-sitter integration for accurate AST-level extraction (see Phase 2)
- Storage: new `symbols` table in `index.db`
- Import extraction: parse `use`, `import`, `require`, `mod` statements

### 4.3 Dependency Graph

**Responsibilities:**
- Map file-to-file relationships
- Track module imports and exports
- Enable impact analysis ("which files import this symbol?")
- Support transitive dependency traversal

**Input:** Import/extract statements from Symbol Index  
**Output:** `Vec<Dependency { from_file, to_file, import_type, line }>`

**Design notes:**
- Directed graph (file → files it imports)
- Store as `dependencies` table: `(id, source_file, target_file, import_type, line_number)`
- Traversal uses BFS/DFS with cycle detection
- Enables inverse queries: "what imports this file?"

### 4.4 Semantic Search

**Responsibilities:**
- Generate embeddings for code chunks
- Perform similarity search against query embeddings
- Bridge natural language questions to code concepts

**Input:** Text query + top-k count  
**Output:** `Vec<SearchResult { path, start_line, end_line, content, score }>`

**Design notes:**
- Reuses existing `SearchResult` struct shape
- Requires an embedding model — local Ollama with `nomic-embed-text` or similar
- Vector storage: the existing `chunks.embedding BLOB` column (currently unused)
- Similarity: cosine distance on embeddings
- **Not implemented in Phase 1** — requires embedding infrastructure

### 4.5 Context Ranker

**Responsibilities:**
- Rank retrieved context by relevance to the user's request
- Choose most relevant files, required symbols, and dependency chain
- Eliminate duplicate or redundant context

**Input:** User request + raw candidates from search + symbols + deps  
**Output:** `Vec<ContextItem { source, content, relevance_score, priority }>`

**Design notes:**
- Rank signals: FTS5 BM25 score, symbol match strength, dependency distance, recency of modification, conversation history
- Configurable rank weights
- Prefer exact matches over keyword matches over semantic matches

### 4.6 Context Budget Manager

**Responsibilities:**
- Enforce token budget limits per model
- Select highest-ranked context within budget
- Prevent oversized prompts that exceed model context windows

**Input:** Ranked context items + model context window limit  
**Output:** Truncated, prioritized context package

**Design notes:**
- Queries model's context window from `ai::model_manager` (already estimates VRAM)
- Algorithm: greedy selection of highest-ranked items until budget exhausted
- Produces structured output: file path → relevant snippet lines
- `ai::context::build_context_prompt` already exists — this feeds into it

## 5. Model Routing Consideration

The Context Engine produces a structured context package. The **Model Router** (`ai::router`) then selects the optimal model based on request complexity and context size.

### Proposed Routing Rules

| Request Type | Context Size | Recommended Model | Rationale |
|---|---|---|---|
| Simple Q&A | < 2K tokens | Local Ollama (fast, free) | Sufficient for factual lookups |
| Code change request | 2K-16K tokens | DeepSeek / specialized coding model | Needs code understanding |
| Architecture decision | 8K-32K tokens | Claude/GPT/Gemini (strong reasoning) | Complex tradeoffs, larger context |
| Repository analysis | 16K-64K tokens | Largest available model | Full workspace understanding needed |

### Integration Points

- Context Engine output feeds into `ai::router::auto_select_model`
- Router already considers `goal`, `cost_preference`, and provider models
- Context length estimate feeds into `CostEstimate.estimated_tokens`

## 6. Security Requirements

- **Workspace isolation:** All context retrieval operates within the opened workspace boundary. The existing `validate_within_workspace` / `validate_new_path_in_workspace` gate applies to any file read path.
- **No external upload by default:** Embeddings are computed locally and stored in the local `index.db`. No data leaves the machine during indexing or retrieval.
- **Local-first indexing:** All indexing is on-device. The embedding model (when implemented) runs via local Ollama, not a cloud API.
- **User approval before modifications:** The governance pipeline (requirement → task → approval → execution) remains the gate for any write operation. Context retrieval is read-only and does not require approval.
- **Dependency graph privacy:** Dependency data stays local. No repository structure metadata is transmitted externally.

## 7. Implementation Phases

### Phase 1: Improve Existing Indexer (Current Session)

**Scope:** Strengthen the current FTS5 indexer without adding new capabilities.

**Actions:**
- Add language detection metadata to chunks
- Index configuration files, documentation, and build files alongside source code
- Add periodic re-indexing trigger (not just manual)
- **Status:** NOT STARTED — this ADR defines the plan

### Phase 2: Add Symbol Extraction

**Scope:** Parse source files for function, struct, class, and import definitions.

**Actions:**
- Implement RegExp-based extraction for `.rs` and `.ts` files
- Create `symbols` table in `index.db`
- Populate symbol index during workspace indexing
- Extend search to support symbol-scoped queries

**Dependencies:** Phase 1 improvements  
**Risk:** Low — RegExp parsing is fast and self-contained  
**Estimated size:** ~300 lines of Rust

### Phase 3: Add Dependency Graph

**Scope:** Extract import/use/mod statements and build a queryable dependency graph.

**Actions:**
- Parse import statements for all detected languages
- Create `dependencies` table in `index.db`
- Implement dependency traversal (direct and transitive)
- Add "reverse dependencies" query (what imports this file?)
- Surface in UI as file relationship explorer

**Dependencies:** Phase 2 (needs symbol/index extraction)  
**Risk:** Medium — cross-language import syntax varies  
**Estimated size:** ~400 lines of Rust + UI bindings

### Phase 4: Add Semantic Retrieval

**Scope:** Implement embedding generation and vector similarity search.

**Actions:**
- Verify Ollama embedding model availability (`nomic-embed-text` or compatible)
- Generate embeddings for all indexed chunks
- Populate `chunks.embedding BLOB` column
- Implement cosine similarity search
- Add hybrid (FTS5 + vector) search mode

**Dependencies:** Phase 1, Ollama with embeddings enabled  
**Risk:** High — embedding quality varies by model; vector search adds significant compute  
**Estimated size:** ~500 lines of Rust

### Phase 5: Add Automatic Context Injection

**Scope:** Automatically retrieve and inject relevant context during chat interactions.

**Actions:**
- Wire Context Engine into `ai::get_context_for_query`
- Implement Context Ranker with configurable weights
- Implement Context Budget Manager
- Add progress indicators to UX during context assembly
- Enable automatic context injection toggle in Settings

**Dependencies:** Phases 2-4 (symbols + deps + semantic)  
**Risk:** Medium — ranking quality determines user trust  
**Estimated size:** ~600 lines of Rust + ~200 lines TypeScript

### Phase 6: Integrate with Composer Mode

**Scope:** Enable multi-file reasoning with full context awareness.

**Actions:**
- Context Engine returns cross-file context package
- DAG planner consumes context to decompose multi-file tasks
- Composer UI shows relevant files, symbols, and dependencies
- Automatic file-granularity task decomposition

**Dependencies:** Phase 5, Composer Mode design (future sprint)  
**Risk:** High — integration surface is large  
**Estimated size:** ~1000 lines total across stack

## 8. Architectural Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Symbol extraction precision | Start with RegExp, graduate to tree-sitter | RegExp is zero-dependency, tree-sitter can be added later without schema change |
| Embedding storage | `chunks.embedding BLOB` (existing column) | Schema already supports it — no migration needed |
| Search hybrid mode | FTS5 + vector, not vector-only | FTS5 is proven and functional; vector adds capability without replacing |
| Dependency graph storage | SQLite adjacency table | Queryable with standard SQL; no graph DB dependency needed |
| Context ranking | Weighted sum of feature scores | Simple, interpretable, easy to tune |
| Token budget strategy | Greedy selection by relevance score | Optimal for the budget; O(n log n) ranking cost is acceptable |

## 9. Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Embedding quality too low for meaningful search | Medium | Medium | Start with FTS5-only, validate embeddings independently before integrating |
| Symbol extraction misses edge cases | Medium | Low | RegExp is heuristic; tree-sitter planned as upgrade path |
| Dependency graph becomes stale | Medium | Low | Re-index on file save (watch mode) |
| Context budget manager truncates critical context | Low | High | Surface truncated items in debug output; tune ranking weights |
| No embedding model available in environment | Medium | Medium | Phase 4 is optional — system works without it (FTS5-only fallback) |

## 10. Implementation Order (Dependency Chain)

```
Phase 1 (Indexer improvements)
    │
    ▼
Phase 2 (Symbol extraction) ──────────────────┐
    │                                           │
    ▼                                           │
Phase 3 (Dependency graph)                     │
    │                                           │
    ▼                                           ▼
Phase 4 (Semantic retrieval)          Phase 5 (Context injection)
    │                                           │
    └───────────────────┬───────────────────────┘
                        │
                        ▼
               Phase 6 (Composer Mode)
```

- Phases 1-3: Independent of external services (Ollama); can be completed without embedding support
- Phase 4: Requires Ollama with embeddings enabled; can be delayed without blocking Phases 5-6
- Phase 5: Can use FTS5-only context if Phase 4 is incomplete
- Phase 6: Requires Phase 5; is the integration goal

## Status

Planned — architecture designed, no implementation begun.