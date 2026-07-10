# ADR-003: Future Agent Architecture Expansion

**Status:** Planned  
**Date:** 2026-07-10  
**Author:** Cline (AI Agent)  

## Current State

Single worker/agent type: **Coder** only.

The capability matching machinery (`intelligence::matcher`), worker registry (`intelligence::registry`), and reliability layer (`intelligence::reliability`) all support multiple agent types but have only one profile to route to. This means the routing, scoring, and retry infrastructure is built and tested but not load-bearing.

## Future Target

A specialized multi-agent system where each agent type has a distinct capability profile, and task routing selects the best-fit agent per requirement.

### Proposed Agents

#### Architect Agent
- **Capabilities:** `repository_analysis`, `architecture_planning`, `dependency_evaluation`
- **Responsibilities:**
  - Analyze repository structure before changes
  - Generate implementation plans decomposed into file-level tasks
  - Evaluate dependency impacts of proposed changes
- **Routing trigger:** Tasks that begin with "analyze", "plan", "design", or where no specific file path is identified

#### Coding Agent (existing, to be retained)
- **Capabilities:** `file_edit`, `code_generation`, `code_review`
- **Responsibilities:**
  - Approved implementation of file changes
  - Controlled, verified file modifications
  - Human-gated execution (existing safety model)
- **Routing trigger:** Tasks that target specific file paths with clear edit objectives

#### Testing Agent
- **Capabilities:** `test_generation`, `regression_detection`, `coverage_analysis`
- **Responsibilities:**
  - Generate validation tests alongside code changes
  - Detect regressions in existing behavior
  - Report coverage gaps
- **Routing trigger:** Tasks that contain "test", "validate", "verify", or are created as companion to a Coding task

#### Review Agent
- **Capabilities:** `security_review`, `risk_analysis`, `compliance_check`
- **Responsibilities:**
  - Review proposed changes for security vulnerabilities
  - Cross-reference against governance requirements
  - Flag risky patterns before they reach production
- **Routing trigger:** Tasks that require approval after initial planning phase

## Decision

Do **NOT** implement the multi-agent system until all of the following prerequisites are met:

1. **Context Engine exists** — Agents need structured, queryable workspace context beyond FTS5 keyword search. A context engine that understands relationships between files, recent changes, and intent is required before agents can make autonomous architecture decisions.

2. **Model Router exists** — Different agent types may benefit from different LLM models (e.g., Architect uses a stronger reasoning model, Reviewer uses a safety-focused model). The router (`ai::router`) supports this conceptually but has no real model diversity to route between.

3. **Repository indexing is production stable** — The FTS5 indexer (`database::indexer`) works but has not been validated under large-scale, long-running workloads. Agents that depend on search results (every agent type does) need a reliable, fast index.

## Reason

Agents without strong context create unreliable autonomous behavior. Deploying multiple agent types before the supporting infrastructure is stable would multiply failure modes rather than distributing work effectively. The single Coder agent should remain the only agent type until the foundation systems are hardened.

## Implementation Order (When Prerequisites Are Met)

1. Register second agent profile (e.g., Tester) in worker registry
2. Update capability strings in `intelligence::registry`
3. Wire routing logic in `intelligence::matcher` to prefer capability-matched agents
4. Add UI affordances for agent selection in `components/AgentPanel.tsx`
5. Add third and fourth agent types incrementally
6. Validate full routing in end-to-end tests

## Status

Planned — no implementation work started. Prerequisites tracked as Sprint 12+ items.