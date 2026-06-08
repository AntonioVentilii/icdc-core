# AI Assistance Entry Point

This is the canonical bootstrap for any AI agent interacting with the `icdc-core` repository. It provides high-level context and directs you to the authoritative governance and intelligence files.

## Project Purpose

ICDC (Internet Computer Derivatives Clearing) is a decentralized, multi-canister clearing infrastructure. It functions as a Central Counterparty (CCP) for derivative instruments, emphasizing risk integrity, position portability, and economic safety.

- **Primary Source**: [README.md](file:///Users/antonio.ventilii/projects/icdc-core/README.md)
- **Technical Overview**: [docs/ai/overview.md](file:///Users/antonio.ventilii/projects/icdc-core/docs/ai/overview.md)

## Governance & Safety

The repository operates under a multi-layered governance model. Agents MUST verify policies and boundaries before making changes.

- **Governance Model**: [docs/ai/governance.md](file:///Users/antonio.ventilii/projects/icdc-core/docs/ai/governance.md)
- **Mandatory Policies**: [.policies/](file:///Users/antonio.ventilii/projects/icdc-core/.policies/)
- **Protected Boundaries**: [.boundaries/](file:///Users/antonio.ventilii/projects/icdc-core/.boundaries/)
- **Agent Capabilities**: [.capabilities/](file:///Users/antonio.ventilii/projects/icdc-core/.capabilities/)

## Architecture & Implementation

The system is built with a focus on modularity and bounded contexts.

- **Architecture Guide**: [docs/ai/architecture.md](file:///Users/antonio.ventilii/projects/icdc-core/docs/ai/architecture.md)
- **Flow Schematics**: [docs/architecture/flows.md](file:///Users/antonio.ventilii/projects/icdc-core/docs/architecture/flows.md)
- **Implementation Rules**: [.agents/](file:///Users/antonio.ventilii/projects/icdc-core/.agents/)
  - [General Instructions](file:///Users/antonio.ventilii/projects/icdc-core/.agents/instructions.md)
  - [Patterns & Conventions](file:///Users/antonio.ventilii/projects/icdc-core/.agents/patterns.md)
  - [Domain Logic](file:///Users/antonio.ventilii/projects/icdc-core/.agents/domain_logic.md)
- **Stable-State Migrations**: [docs/ai/migrations.md](file:///Users/antonio.ventilii/projects/icdc-core/docs/ai/migrations.md)

## Quality & Testing

All changes must adhere to established testing and quality standards.

- **Testing Standards**: [.workflows/testing-standards.md](file:///Users/antonio.ventilii/projects/icdc-core/.workflows/testing-standards.md)
- **Testing Policy**: [.policies/testing-policy.md](file:///Users/antonio.ventilii/projects/icdc-core/.policies/testing-policy.md)
- **PR & CI Conventions**: [docs/ai/pr-and-ci.md](file:///Users/antonio.ventilii/projects/icdc-core/docs/ai/pr-and-ci.md)

## Agent Coordination

Refer to the following for role-specific guidance and tool-optimized instructions.

- **Agent Registry**: [AGENTS.md](file:///Users/antonio.ventilii/projects/icdc-core/AGENTS.md)
- **Claude Optimization**: [CLAUDE.md](file:///Users/antonio.ventilii/projects/icdc-core/CLAUDE.md)
- **Standard Workflows**: [.workflows/](file:///Users/antonio.ventilii/projects/icdc-core/.workflows/)
