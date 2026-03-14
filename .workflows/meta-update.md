---
description: Workflow for updating AI governance and configuration files
---

This workflow must be followed whenever an AI agent needs to update the repository's AI governance layer (files in `.agents/`, `.policies/`, `.boundaries/`, `.workflows/`, or `AGENTS.md`).

## Workflow Steps

1. **Identify the Change**: Determine if the current task introduces new patterns, architectural shifts, or domain boundary changes.
2. **Locate Target Files**: Identify which AI guidance files are affected (e.g., `.agents/patterns.md`, `.policies/code-modification.md`).
3. **Draft the Update**: Write the updated sections or new files. Ensure consistency with existing policies.
   // turbo
4. **Self-Check Compliance**: Verify that the proposed update complies with [.policies/ai-meta-updates.md](file:///Users/antonio.ventilii/projects/icdc-core/.policies/ai-meta-updates.md).
5. **Include in PR**: Include the governance updates in the same PR/Submission as the code changes.
6. **Request Review**: Explicitly mention the AI governance changes in the PR description to ensure human reviewers are aware.

## Verification

- Run `ls -R .agents .policies .boundaries .workflows` to ensure no orphaned files.
- Verify all links are functional.
