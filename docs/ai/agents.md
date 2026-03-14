# Agent Ecosystem

This document explains the roles, responsibilities, and coordination model of the agents operating in this repository.

## Agent Roles

### [Planner](file:///Users/antonio.ventilii/projects/icdc-core/.agents/planner.md)

The **Architect**. Responsible for:

- Analyzing user requests and decomposing them into technical tasks.
- Ensuring alignment with repository architecture and sharding plans.
- Generating and maintaining the `implementation_plan.md`.

### [Implementer](file:///Users/antonio.ventilii/projects/icdc-core/.agents/implementer.md)

The **Builder**. Responsible for:

- Modifying files and writing code based on an approved plan.
- Adhering to established [Implementation Patterns](file:///Users/antonio.ventilii/projects/icdc-core/.agents/patterns.md).
- Ensuring code is maintainable and follows repository-specific idioms.

### [Reviewer](file:///Users/antonio.ventilii/projects/icdc-core/.agents/reviewer.md)

The **Guardian**. Responsible for:

- Verifying code correctness and technical quality.
- Enforcing **Economic Safety** and atomicity rules.
- Validating that changes do not violate [Policies](file:///Users/antonio.ventilii/projects/icdc-core/.policies/) or [Boundaries](file:///Users/antonio.ventilii/projects/icdc-core/.boundaries/).

## Coordination Model

Agents do not work in isolation. They follow a collaborative loop:

1. **Planning**: Planner creates a design.
2. **Review**: Human/Reviewer approves the design.
3. **Execution**: Implementer writes the code.
4. **Verification**: Reviewer validates the implementation.

## Authority and Handoffs

- Authority is derived from the **Governance Layer**.
- Handoffs are documented via artifacts (`task.md`, `implementation_plan.md`, `walkthrough.md`).
- Agents must respect the boundaries of other roles (e.g., an Implementer should not redefine architecture without a Planner).

## Authoritative Files

- [AGENTS.md](file:///Users/antonio.ventilii/projects/icdc-core/AGENTS.md): The main registry.
- [.agents/](file:///Users/antonio.ventilii/projects/icdc-core/.agents/): Structured definitions for each role.
