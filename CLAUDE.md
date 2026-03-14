# Claude Behavior Guidelines

This file contains instructions specifically optimized for the Claude model. Claude should follow these rules to ensure consistency and predictability.

## Reasoning Preferences

- **Think Step-by-Step**: Always use `<thought>` blocks for complex reasoning.
- **Explain Tradeoffs**: When proposing architectural changes, explicitly list pros and cons.
- **Incremental Progress**: Prefer small, verifiable patches over massive rewrites.

## Coding Rules

- **Targeted Edits**: Use `multi_replace_file_content` or `replace_file_content` for precise changes.
- **Follow Patterns**: Strictly adhere to the [Implementation Patterns](file:///Users/antonio.ventilii/projects/icdc-core/.agents/patterns.md).
- **Quality First**: Never bypass `npm run quality` checks.
- **Governance First**: Check [.policies/](file:///Users/antonio.ventilii/projects/icdc-core/.policies/) and [.boundaries/](file:///Users/antonio.ventilii/projects/icdc-core/.boundaries/) before making modifications.

## Repository Conventions

- **Rust**: Idiomatic code, no redundant comments, proper error handling.
- **Naming**: Use descriptive, snake_case identifiers for functions and variables.
- **Documentation**: Keep `walkthrough.md` and `task.md` updated during execution.

## Coordination

**CRITICAL: Start with [AI.md](file:///Users/antonio.ventilii/projects/icdc-core/AI.md) for the canonical repository bootstrap.**

Refer to [AGENTS.md](file:///Users/antonio.ventilii/projects/icdc-core/AGENTS.md) for the general agent registry and [docs/ai/governance.md](file:///Users/antonio.ventilii/projects/icdc-core/docs/ai/governance.md) for the repository governance system.
