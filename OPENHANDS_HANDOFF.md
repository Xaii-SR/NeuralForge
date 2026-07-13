# NEURALFORGE OPENHANDS HANDOFF

## IMPORTANT

This is an existing development project.

This is NOT a new project.

The previous AI developer completed Sprint 0 and Sprint 1.

Continue development from the current repository state.

Do not rebuild.

Do not redesign.

Do not restart architecture discovery.

Your job is continuation.

---

# PROJECT

## Name

NeuralForge


## Purpose

NeuralForge is a local-first AI development environment similar to Cursor AI and Claude Code.

The goal is an AI engineering assistant capable of:

- Understanding a codebase
- Planning changes
- Editing files
- Running verification
- Rolling back failed changes
- Maintaining engineering traceability
- Eventually performing autonomous engineering workflows


---

# TECH STACK


## Desktop

Tauri


## Backend

Rust


## Frontend

Next.js

React

TypeScript


## Database

SQLite


## AI

Provider abstraction.

Current provider:

Ollama


---

# CURRENT PROJECT STRUCTURE


Important directories:


```
src-tauri/

Rust backend


app/

Next.js application


components/

React components


lib/

Frontend IPC wrappers


hooks/

Frontend hooks


.neuralforge/

Workspace data


```

---

# DEVELOPMENT HISTORY


# Sprint 0

STATUS:

✅ COMPLETE


Purpose:

Repository audit and architecture mapping.


Completed:

- Existing architecture analyzed
- Agent system mapped
- Database structure documented
- Safe migration strategy created
- Existing test baseline verified


Baseline:

```
cargo test --lib
```


Result:

```
77 passed
0 failed
7 ignored
```


---

# Sprint 1

STATUS:

✅ COMPLETE


Name:

Requirement Intelligence Layer


Purpose:

Prevent vague AI requests from reaching the planner.


Before Sprint 1:


User request:

"make it better"


would directly enter AI planning.


After Sprint 1:


```
User Request

↓

Requirement Validation

↓

RequirementContract

↓

Agent Planning

↓

Execution
```


---

# Sprint 1 IMPLEMENTATION


Created:


```
src-tauri/src/governance/

    mod.rs

    requirements.rs

    validator.rs
```


Frontend:


```
lib/governance.ts
```


---

# Requirement System


RequirementContract contains:


- id
- version
- title
- intent
- acceptance criteria
- status
- correlation_id
- timestamps
- created_by


---

# DATABASE AFTER SPRINT 1


Database:

```
.neuralforge/index.db
```


Added tables:


## requirements


Stores:

- requirement data
- version
- status
- correlation_id


## requirement_history


Stores:

- version history
- status changes


## agent_tasks additions


Nullable fields:


```
requirement_id

correlation_id
```


---

# CURRENT AGENT FLOW


Current workflow:


```
AgentPanel

↓

Create Requirement

↓

validator::validate

↓

RequirementContract

↓

create_and_plan_task(requirement_id)

↓

planner

↓

Human Approval

↓

executor

↓

Verification

↓

Rollback if failure
```


---

# TEST STATUS


Current Sprint 1 baseline:


```
cargo test --lib
```


Result:


```
90 passed
0 failed
7 ignored
```


This baseline must not regress.


---

# IMPORTANT ARCHITECTURE RULES


The following systems are already stable:


```
agent/executor.rs

agent/planner.rs

agent/memory.rs
```


Do NOT modify unless absolutely required by Sprint 2.


---

## executor.rs

Responsible for:


- File snapshots
- File writes
- Verification
- Rollback


---

## planner.rs

Responsible for:


- AI planning only


---

## memory.rs

Responsible for:


- agent_history.md logging


Sprint 2 must not replace this system.


---

# DATABASE SAFETY RULES


Sprint 2 database changes are additive only.


Do NOT:


- Delete tables
- Rename tables
- Rewrite existing rows
- Create a second database
- Introduce an ORM
- Introduce a migration framework


Continue using:


```
SQLite

rusqlite

existing SCHEMA constant pattern
```


Migration style:


```
CREATE TABLE IF NOT EXISTS
```


---

# CURRENT DEVELOPMENT POSITION


Completed:


```
Sprint 0 ✅

Sprint 1 ✅
```


Current task:


```
Sprint 2
```


---

# SPRINT 2


Name:

Traceability Ledger + Evidence System


Goal:


Create complete engineering traceability:


```
Requirement

↓

Task

↓

Approval

↓

Execution

↓

Verification

↓

Evidence
```


Everything is connected through:


```
correlation_id
```


---

# SPRINT 2 FILES


## Create


```
src-tauri/src/governance/ledger.rs

src-tauri/src/governance/evidence.rs
```


---

## Modify


```
src-tauri/src/governance/mod.rs

src-tauri/src/governance/requirements.rs

src-tauri/src/agent/mod.rs

src-tauri/src/database/mod.rs

src-tauri/src/lib.rs

src-tauri/Cargo.toml

lib/governance.ts
```


---

# DATABASE CHANGES


Add:


## ledger_entries


Schema:


```sql
seq INTEGER PRIMARY KEY AUTOINCREMENT

event_type TEXT NOT NULL

correlation_id TEXT

requirement_id TEXT

task_id TEXT

payload TEXT NOT NULL

created_at INTEGER NOT NULL

prev_hash TEXT NOT NULL

entry_hash TEXT NOT NULL
```


---

Add:


## evidence


Schema:


```sql
id TEXT PRIMARY KEY

task_id TEXT NOT NULL

correlation_id TEXT

kind TEXT NOT NULL

content TEXT NOT NULL

success BOOLEAN NOT NULL DEFAULT 0

created_at INTEGER NOT NULL
```


The `success` field is required.

Reason:

Future Sprint 5 worker reputation system.


---

# LEDGER REQUIREMENTS


The ledger must be:


- Append only
- Queryable
- Correlation based
- Hash chained


Use:


```
SHA-256
```


Hash calculation:


```
SHA256(
previous_hash +
sequence +
event_type +
correlation_id +
requirement_id +
task_id +
payload +
timestamp
)
```


Genesis hash:


```
64 zeros
```


---

# LEDGER EVENT SYSTEM


Create a LedgerEvent enum.


Do NOT scatter string literals throughout the project.


Events:


```
requirement_created

requirement_updated

requirement_retired

requirement_rejected

task_created

task_planned

task_plan_failed

task_approved

task_completed

task_failed

task_rolled_back

task_rejected
```


The enum must serialize exactly to snake_case.


Example:


Rust:


```
LedgerEvent::TaskApproved
```


Output:


```
task_approved
```


---

# CORRELATION RULE


The correlation_id lifecycle:


Requirement creation:


```
CREATE correlation_id
```


Task creation:


```
COPY correlation_id
```


Evidence:


```
COPY correlation_id
```


Ledger:


```
REFERENCE correlation_id
```


Never create new correlation IDs downstream.


---

# TAMPER DETECTION REQUIREMENT


The tamper test MUST:


1. Insert real ledger entries.


2. Modify SQLite directly using raw SQL.


Example:


```sql
UPDATE ledger_entries SET payload='tampered'
```


3. Run:


```
verify_chain()
```


4. Confirm failure.


Do NOT create a mocked tamper test.


The test must prove the hash chain detects database modification.


---

# REJECTED REQUIREMENTS


Maintain Sprint 1 behavior.


Invalid requirements:


DO NOT create requirement rows.


Instead create:


```
requirement_rejected
```


ledger event.


Rules:


```
correlation_id = NULL
```


Reason:

No requirement lifecycle existed.


Payload contains:


- submitted title
- submitted intent
- validation errors


Limits:


```
title maximum 500 characters

intent maximum 500 characters
```


---

# RUN_CODE


Do NOT redesign run_code.


Do NOT add requirement gating.


Sprint 3 handles run_code governance.


Sprint 2 only records ledger events.


For run_code:


```
requirement_id = NULL
```


---

# AGENT HISTORY


Do not modify:


```
memory.rs
```


Do not replace:


```
agent_history.md
```


The ledger becomes the structured source of truth.


agent_history.md remains a human-readable convenience log.


---

# TEST REQUIREMENTS


Before completion:


Run:


```
cargo test --lib
```


Must preserve:


```
90 existing tests
```


Add tests for:


- Ledger creation
- Genesis hash
- Hash chaining
- Evidence storage
- Correlation queries
- Tamper detection
- Requirement events
- Agent lifecycle events


---

# COMPLETION REQUIREMENT


After Sprint 2:


STOP.


Do not begin Sprint 3.


Create final report containing:


- Files changed
- Database changes
- Architecture changes
- Tests added
- Tests passed
- Regression status
- Remaining issues


Wait for approval before continuing.