# AI Workflows

This document explains the coordinated processes and multi-agent coordination patterns used in this repository.

## Purpose

Complex tasks require more than one agent and a series of structured steps to ensure safety and quality. Workflows define these sequences to make automation repeatable and predictable.

## Key Workflows

### Feature Development

A multi-stage process for adding new functionality:

1. **Decomposition**: Analysis of boundaries and dependencies.
2. **Planning**: Technical design and goal definition.
3. **Drafting**: Incremental implementation of the feature.
4. **Hardening**: Testing and documentation updates.

### Bug Fixing

A systematic approach to resolving issues:

1. **Reproduction**: Creating a failing test case.
2. **Analysis**: Identifying the root cause.
3. **Patching**: Implementing the fix.
4. **Verification**: Ensuring the fix works and no regressions were introduced.

### Specialized Workflows

The repository also maintains structured definition files for specific technical tasks:

- [New API Endpoint Workflow](file:///Users/antonio.ventilii/projects/icdc-core/.workflows/new-api-endpoint.md)
- [Testing Standards Workflow](file:///Users/antonio.ventilii/projects/icdc-core/.workflows/testing-standards.md)

## Operational Definitions

While this document explains the concepts, the actual executable steps for agents are defined in the **[.workflows/](file:///Users/antonio.ventilii/projects/icdc-core/.workflows/)** directory.

Agents are expected to read and follow the relevant workflow file for every significant task they undertake.
