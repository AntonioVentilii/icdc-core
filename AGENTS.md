# AI Agents Registry (ICDC Core)

This document defines the roles, responsibilities, and boundaries of automated agents operating within this repository.

## Purpose

Agent configuration files exist to make automated agents predictable, constrained, and interoperable. This file serves as the **Table of Contents of Automation**.

See [AI.md](file:///Users/antonio.ventilii/projects/icdc-core/AI.md) for the canonical repository bootstrap.

See [docs/ai/governance.md](file:///Users/antonio.ventilii/projects/icdc-core/docs/ai/governance.md) for the repository AI governance system.

## Governance

This repository follows a multi-layered governance model to ensure AI agents operate safely and predictably.

- **[.policies/](file:///Users/antonio.ventilii/projects/icdc-core/.policies/)**: Repository-wide laws and constraints.
- **[.capabilities/](file:///Users/antonio.ventilii/projects/icdc-core/.capabilities/)**: Explicit permissions and restrictions for agents.
- **[.boundaries/](file:///Users/antonio.ventilii/projects/icdc-core/.boundaries/)**: Protected areas and sensitive modules.
- **[.workflows/](file:///Users/antonio.ventilii/projects/icdc-core/.workflows/)**: Standard processes for agent coordination.

## Agent Roles

### [Planner](file:///Users/antonio.ventilii/projects/icdc-core/.agents/planner.md)

Responsible for task decomposition, architectural alignment, and implementation planning.

### [Implementer](file:///Users/antonio.ventilii/projects/icdc-core/.agents/implementer.md)

Writes code, modifies files, and ensures technical compliance with established patterns.

### [Reviewer](file:///Users/antonio.ventilii/projects/icdc-core/.agents/reviewer.md)

Ensures quality, verifies tests, and enforces atomicity and economic safety rules.

## Core Guidance

- [General Instructions](file:///Users/antonio.ventilii/projects/icdc-core/.agents/instructions.md)
- [Architecture & Sharding](file:///Users/antonio.ventilii/projects/icdc-core/.agents/architecture.md)
- [Domain Logic & Atomicity](file:///Users/antonio.ventilii/projects/icdc-core/.agents/domain_logic.md)
- [Implementation Patterns](file:///Users/antonio.ventilii/projects/icdc-core/.agents/patterns.md)

## Specialized Workflows

- [New API Endpoint Workflow](file:///Users/antonio.ventilii/projects/icdc-core/.workflows/new-api-endpoint.md)
- [Testing Standards Workflow](file:///Users/antonio.ventilii/projects/icdc-core/.workflows/testing-standards.md)

## Coordination Rules

1. **Minimal Duplication**: Information should exist in only one place (Registry -> Documentation -> Model-Specific).
2. **Tool-Specific Isolation**: Model-specific behaviors belong in `CLAUDE.md` or model-specific directories.
3. **Plan First**: Complex changes always require an `implementation_plan.md`.
4. **Governance First**: AI agents must verify [Policies](file:///Users/antonio.ventilii/projects/icdc-core/.policies/) and [Boundaries](file:///Users/antonio.ventilii/projects/icdc-core/.boundaries/) before making changes.
5. **Meta-Updates**: Fundamental changes must update relevant AI guidance in `.agents/` per [.policies/ai-meta-updates.md](file:///Users/antonio.ventilii/projects/icdc-core/.policies/ai-meta-updates.md).
