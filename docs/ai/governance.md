# AI Governance Layer

This repository defines a governance layer used by automated agents and AI tools.

The governance layer provides rules, capabilities, and coordination patterns that automated systems must follow when modifying the repository.

## Governance Model

The model is divided into four operational areas, each with a specific scope and purpose.

### [.policies](file:///Users/antonio.ventilii/projects/icdc-core/.policies/) - The Laws

Defines repository-wide rules and constraints. Policies are **mandatory** and represent the ground truth for what is forbidden or required.

- **Scope**: Security, testing, code quality, dependency management.
- **Interpretation**: Non-negotiable constraints.

### [.capabilities](file:///Users/antonio.ventilii/projects/icdc-core/.capabilities/) - The Permissions

Defines what automated agents are specifically allowed or forbidden to do.

- **Scope**: Editing scope, infrastructure access, documentation authority.
- **Interpretation**: Scope of legitimate action for an agent.

### [.workflows](file:///Users/antonio.ventilii/projects/icdc-core/.workflows/) - The Processes

Defines multi-agent processes and coordination patterns.

- **Scope**: Feature development, bug fixing, API standards.
- **Interpretation**: Standard Operating Procedures (SOPs).

### [.boundaries](file:///Users/antonio.ventilii/projects/icdc-core/.boundaries/) - The Protected Zones

Defines sensitive or restricted areas of the repository.

- **Scope**: Infrastructure, core security modules, ledger state.
- **Interpretation**: Areas where automation should be minimized or highly cautious.

## Relationship Hierarchy

1. **Policies** override all other guidance.
2. **Boundaries** restrict where actions can happen.
3. **Capabilities** define what actions can happen.
4. **Workflows** define the "how-to" sequence.

Agents must verify policies and boundaries before initiating any significant change.
