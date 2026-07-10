// Auto-generated type definitions.
// Source: Rust specta derives on backend structs.
// Do not edit manually. Regenerate via:
//   cargo test -p app test_export_types -- --nocapture

// Governance types
export type RequirementContract = {
  id: string;
  version: number;
  title: string;
  intent: string;
  acceptance_criteria: Array<string>;
  status: string;
  correlation_id: string;
  created_at: number;
  updated_at: number;
  created_by: string;
};

export type RequirementHistoryEntry = {
  requirement_id: string;
  version: number;
  status: string;
  title: string;
  intent: string;
  acceptance_criteria: Array<string>;
  changed_at: number;
};

export type LedgerEntry = {
  seq: number;
  event_type: string;
  correlation_id: string | null;
  requirement_id: string | null;
  task_id: string | null;
  payload: string;
  created_at: number;
  prev_hash: string;
  entry_hash: string;
};

export type ChainVerification = {
  valid: boolean;
  entries: number;
  problem: string | null;
};

export type EvidenceRecord = {
  id: string;
  insertion_sequence: number;
  task_id: string;
  correlation_id: string | null;
  kind: string;
  content: string;
  success: boolean;
  created_at: number;
};

export type PromotionRequest = {
  id: string;
  evidence_id: string;
  task_id: string;
  status: string;
  requested_at: number;
  promoted_at: number | null;
};

// Agent types
export type AgentTask = {
  id: string;
  objective: string;
  agent: string;
  task_type: string;
  files: Array<string>;
  status: string;
  verification: string | null;
  rollback: string | null;
  proposed_content: string | null;
  risk_summary: string | null;
  error: string | null;
  requirement_id: string | null;
  correlation_id: string | null;
  dag_id: string | null;
  depends_on: Array<string>;
  retry_of: string | null;
};

// File system types
export type FileEntry = {
  name: string;
  path: string;
  is_dir: boolean;
};

// Planning types
export type PlannedTask = {
  id: string;
  file_path: string;
  note: string | null;
  depends_on: Array<number>;
};

// Database types
export type FileCandidate = {
  path: string;
  score: number;
  match_kind: string;
};

// Intelligence types
export type WorkerProfile = {
  id: string;
  name: string;
  capabilities: Array<string>;
  reliability_score: number;
  tasks_completed: number;
  tasks_failed: number;
};

export type FailureClass =
  | "compile_error"
  | "test_failure"
  | "execution_error"
  | "blocked_dependency"
  | "user_rejected"
  | "unknown"
  | "not_failed";

export type CompletenessReport = {
  complete: boolean;
  missing: Array<string>;
};

export type ConfidenceReport = {
  score: number;
  factors: Array<string>;
};

// Extension types
export type ExtensionManifest = {
  name: string;
  version: string;
  author: string;
  description: string;
  entry_point: string;
  runtime: string;
  permissions: Array<string>;
};

export type InstalledExtension = {
  manifest: ExtensionManifest;
  dir: string;
  enabled: boolean;
};

export type ExtensionResult = {
  success: boolean;
  output: unknown;
  error: string | null;
};

// Provider types
export type ProviderId =
  | "ollama"
  | "open_ai"
  | "anthropic"
  | "gemini"
  | "deep_seek"
  | "groq"
  | "mistral"
  | "codestral"
  | "claude_code"
  | "grok"
  | "generic_openai";

// Hardware types
export type CpuInfo = {
  brand: string;
  physical_cores: number;
  logical_cores: number;
  frequency_mhz: number;
};

export type MemoryInfo = {
  total_mb: number;
  available_mb: number;
};

export type GpuInfo = {
  name: string;
  vendor: string;
  vram_mb: number;
  utilization_percent: number | null;
};

export type HardwareInfo = {
  cpu: CpuInfo;
  memory: MemoryInfo;
  gpus: Array<GpuInfo>;
};

// AI types
export type OllamaModel = {
  name: string;
  size_bytes: number;
  parameter_size: string;
  quantization_level: string;
  context_length: number;
  family: string;
};

export type ProviderMetadata = {
  id: string;
  name: string;
  is_local: boolean;
  requires_api_key: boolean;
  configured: boolean;
};

export type ProviderHealthInfo = {
  provider: string;
  healthy: boolean;
  avg_latency_ms: number | null;
  failure_count: number;
  cooldown_seconds_remaining: number | null;
};

export type VramCheckResult = {
  sufficient: boolean;
  required_mb: number;
  available_mb: number;
  message: string;
};

export type IndexStats = {
  files_scanned: number;
  files_indexed: number;
  files_skipped_unchanged: number;
  chunks_created: number;
};

export type SearchResult = {
  path: string;
  start_line: number;
  end_line: number;
  content: string;
  score: number;
};

export type ResolutionResult = {
  resolved: string | null;
  candidates: Array<FileCandidate>;
};

export type Preferences = {
  goal: "speed" | "quality";
  cost_preference: "free" | "cheap" | "quality_first";
};

export type CostEstimate = {
  estimated_tokens: number;
  estimated_cost_usd: number;
  is_free: boolean;
};

export type BenchmarkResult = {
  model: string;
  tokens_per_second: number | null;
  latency_ms: number;
  vram_required_mb: number;
  reliable: boolean;
  benchmarked_at: number;
};

export type AutoSelection = {
  provider: string;
  model: string;
  reason: string;
  estimated_cost_usd: number;
  is_free: boolean;
};

export type ChatMessage = {
  role: "user" | "assistant" | "system";
  content: string;
};

// Bootstrap types
export type SelfImprovementProposal = {
  title: string;
  slug: string;
  file_path: string;
  rationale: string;
  original_content: string;
  proposed_content: string;
  risk_summary: string;
  diff: string;
};

export type SelfImprovementResult = {
  branch_name: string;
  diff: string;
  tests_passed: boolean;
  test_output: string;
  pr_summary: string;
};

// Reliability types
export type RetryDecision = {
  allowed: boolean;
  reason: string;
  failure_class: FailureClass;
  attempts_so_far: number;
  retry_task_id: string | null;
};

export type WorkerMatch = {
  profile: WorkerProfile;
  score: number;
  matched: number;
  missing: Array<string>;
};

export type TaskReport = {
  task: AgentTask;
  failure_class: FailureClass;
  attempts: number;
  lineage: Array<string>;
  evidence: Array<EvidenceRecord>;
  promotions: Array<PromotionRequest>;
  ledger_events: Array<LedgerEntry>;
  confidence: ConfidenceReport;
  completeness: CompletenessReport;
};

// DAG types
export type TaskDagRecord = {
  id: string;
  requirement_id: string;
  version: number;
  created_at: number;
  correlation_id: string;
  task_ids: Array<string>;
  execution_order: Array<string>;
};