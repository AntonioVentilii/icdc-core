# AI Orchestration Architecture

This document describes the technical architecture of the AI orchestration system used in this repository.

## Components

### Context Sources

Agents pull context from several prioritized sources:

1. **Registry**: `AGENTS.md` and `.agents/` define roles.
2. **Behavior Guides**: `CLAUDE.md` provides model-specific instructions.
3. **Governance Layer**: `.policies/` and `.boundaries/` provide constraints.
4. **Project Specs**: `README.md`, `NEW_ARCHITECTURE.md`, and `SHARDING_PLAN.md`.

### Tool Orchestration

Agents use a suite of tools to interact with the repository:

- **File System Tools**: `read_file`, `write_to_file`, `replace_file_content`.
- **Search Tools**: `grep_search`, `find_by_name`.
- **Terminal Tools**: `run_command`, `command_status`.
- **Communication Tools**: `notify_user` (for human-in-the-loop approval).

### The Planning Loop

Every non-trivial change follows a recursive planning architecture:

1. **Decomposition**: Breaking a user request into atomic sub-tasks.
2. **Implementation Plan**: An artifact describing _what_ will change and _how_ it will be verified.
3. **Approval**: Human review of the plan via `notify_user`.
4. **Execution**: Sequential or parallel edits according to the plan.
5. **Walkthrough**: Post-execution summary of changes.

## Decision Hierarchy

1. **Human Override**: Explicit instructions from the user take precedence.
2. **Governance Policies**: Mandatory rules in `.policies/`.
3. **Agent Definitions**: Role-specific constraints in `.agents/`.
4. **General Repository Patterns**: Conventions found in the codebase.

## Reliability and Safety

- **Atomic Edits**: Targeted file changes instead of full overwrites.
- **Verification**: Built-in testing phases (unit and integration tests).
- **Economic Safeguards**: Explicit checks for modules handling ledger and payouts.
