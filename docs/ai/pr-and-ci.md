# PR & CI

Everything an agent needs to open a green PR against `icdc-core`.

This page mirrors the structure of
[`dfinity/control-panel`'s `docs/ai/pr-and-ci.md`](https://github.com/dfinity/control-panel/blob/main/docs/ai/pr-and-ci.md)
and adapts it to icdc-core's Rust-workspace + npm-script stack.

## 1. PR title — Conventional Commits

Use a [Conventional Commits](https://www.conventionalcommits.org/) title:

```
verb(scope): description
verb(scope)!: description   # breaking change
verb: description           # scope optional, see below
```

Examples that match recent history (`git log --oneline`):

- `fix: settlement safety rails (orders, trades, error shape)`
- `refactor: Principal to Grantee`
- `chore: v0.0.7`
- `chore(npm-deps-dev): bump prettier-plugin-motoko from 0.12.5 to 0.13.0`
- `chore(github-actions): bump actions/download-artifact from 7.0.0 to 8.0.1`
- `feat(clearing)!: change settlement signature` ← `!` marks a breaking change

There is currently **no CI gate enforcing the title regex**, but the
convention is what `release.yml` and the changelog tooling rely on, so
treat it as binding. If you add `pr-checks.yml` later, mirror the
control-panel regex
`^(feat|fix|chore|build|ci|docs|style|refactor|perf|test)(\([-a-zA-Z0-9,]+\))?!?:`
(scope optional, since this repo's history uses both forms).

### Verbs

| verb       | when                                                     |
| ---------- | -------------------------------------------------------- |
| `feat`     | new user-visible feature or new public API surface       |
| `fix`      | bug fix                                                  |
| `refactor` | internal change with no behavior change                  |
| `perf`     | performance improvement                                  |
| `docs`     | docs only (incl. `docs/ai/**`, `README.md`, crate docs)  |
| `test`     | tests only                                               |
| `chore`    | misc maintenance (release bumps, dependency hygiene, …)  |
| `build`    | build system / packaging (`scripts/build.*`, `dfx.json`) |
| `ci`       | CI workflows / actions / `dev-tools.json`                |
| `style`    | formatting only — no logic change                        |

### Scope

Single word or comma-separated list of affected areas. Use the
existing vocabulary so it shows up grouped in changelogs.

Common scopes used in this repo:

- Crate names: `clearing`, `registry`, `minter`, `shared`
- Pseudo-areas: `ci`, `build`, `release`, `docs`, `ai`
- Dependabot-style: `npm-deps-dev`, `github-actions`, `rust`,
  `cargo-deps`

If you introduce a new scope, keep it short and lowercase, no spaces.

## 2. PR body — template

Always follow the repo's PR template. The canonical source is
[`.github/pull_request_template.md`](../../.github/pull_request_template.md)
— never invent your own structure, never drop sections, never replace
it with a free-form description. If the template changes, the new
version wins; re-read it before opening a PR.

The template mandates three sections:

```markdown
# Motivation

<!-- Why this change exists. 1-3 sentences. Link the issue / Linear /
     Slack thread if there is one. -->

# Changes

<!-- Bulleted list of what actually changed.
     Be specific about file / crate / function names where relevant.
     If you updated docs/ai/** or .agents/**, mention it here. -->

# Tests

<!-- What you ran locally to validate.
     Examples: `npm run test:unit`, `npm run test:integration`,
     `cargo clippy --tests -p clearing`, manual `dfx deploy --network
     local` smoke check. Screenshots only for visible UI work. -->
```

Rules:

- **All three sections are required.** Don't leave them empty, don't
  merge them, don't rename them.
- **Use the exact section headings** (`# Motivation`, `# Changes`,
  `# Tests`) so downstream tooling (release notes, search) can find
  them.
- **Do not hard-wrap lines.** Write one line per paragraph or list
  item and let the GitHub renderer wrap. Hard-wrapping at ~80 columns
  (a default many models fall back to) breaks rendering inside lists,
  blockquotes, and tables. This applies to the PR body only — source
  files still follow `rustfmt` / `prettier` / `shfmt`.
- **Atomicity statement.** If the PR touches more than one logical
  thing, add a one-liner under `# Motivation` explaining why they
  belong together. If you can't, split the PR.

## 3. Base branch & draft state

Default base branch is **`main`**. Open every PR against `main` unless
the user explicitly tells you to use a different base.

This applies **even when work is split into a wave / stack of related
PRs**. Each PR in the wave still branches from `main` and targets
`main` — never target another open PR's branch as a base.

If a PR depends on another PR that has not landed yet:

- Mark the dependent PR as **draft** on creation.
- Note the dependency in the PR body under `# Motivation` (e.g.
  "Depends on #1234. Will mark ready once that lands.").
- Once the prerequisite merges into `main`, pull `main` into the
  dependent branch (regular merge — no force-push, see
  [§9](#9-updating-an-existing-pr)) and flip the PR to "ready for
  review".

Why this rule exists:

- One review surface per PR. Reviewers always diff against `main`.
- The prerequisite PR can be edited, force-pushed by its author, or
  re-titled without silently breaking dependents.
- `release.yml` and tag-based release flows assume `main` as the merge
  target.

**Draft state** is also the correct status for any PR that is not yet
ready for review — work-in-progress, waiting on an upstream
dependency, smoke-testing in CI. Mark it draft on creation and flip to
"ready for review" only once it stands on its own.

## 4. Atomicity

One logical change per PR. If you catch yourself writing
"and also" / "while I was there" / "I noticed that" in the body, split.

| Anti-pattern                                         | Do this instead                                                        |
| ---------------------------------------------------- | ---------------------------------------------------------------------- |
| "Add feature X and rename old function"              | PR 1: `refactor: rename`. PR 2: `feat: X`.                             |
| "Fix bug Y and update unrelated formatting"          | Two PRs.                                                               |
| "Refactor 5 modules into shared `Foo`"               | PR 1: introduce `Foo` + migrate 1 caller. PR 2..N: migrate the others. |
| "Speed up tests via CI cache + nextest + lazy WASMs" | Three small PRs (one CI change, one test infra change per PR).         |

This is also stated as commandment 2 in
[`.agents/instructions.md`](../../.agents/instructions.md) and is
enforced by the
[reviewer agent](../../.agents/reviewer.md).

When a wave of dependent PRs is genuinely needed, follow the base
branch + draft rule in [§3](#3-base-branch--draft-state) — atomicity
and stacking are complementary, not in conflict.

## 5. Local quality gates (run before opening the PR)

From the repo root:

```bash
# 1. Format and auto-fix lint
npm run quality           # = format + lint (prettier + scripts)
npm run quality:rust      # = rustfmt + clippy --fix

# 2. Run unit tests
npm run test:unit

# 3. Run integration tests (slower; spins up PocketIC)
npm run test:integration
# Or scoped: cargo test --test it -p clearing settlement
#            cargo nextest run --test it -p clearing settlement

# 4. Match CI's strict gates
RUSTFLAGS="-D warnings" cargo test --lib --bins
RUSTFLAGS="-D warnings" cargo test --test it
```

If you regenerated Candid:

```bash
npm run did               # regenerates *.did files for every crate
```

If you've never run integration tests on this machine, install
PocketIC and (recommended) `cargo-nextest` first:

```bash
scripts/pic-install
scripts/setup cargo-nextest
```

## 6. CI jobs you must keep green

Defined under [`.github/workflows/`](../../.github/workflows/):

| Workflow      | Job(s)                      | What it runs                                                                |
| ------------- | --------------------------- | --------------------------------------------------------------------------- |
| `checks.yml`  | `format`                    | `npm run format` — fails if it produces a diff and no PAT is available.     |
| `checks.yml`  | `lint`                      | `npm run lint` — `prettier --check` + `scripts/lint.sh` (rust, did, shell). |
| `tests.yml`   | `unit`                      | `npm run test:unit` (`cargo test --lib --bins`).                            |
| `tests.yml`   | `integration`               | `npm run build` then `npm run test:integration`.                            |
| `tests.yml`   | `tests-pass`                | Aggregator gate via `.github/actions/needs_success`.                        |
| `checks.yml`  | `checks-pass`               | Aggregator gate via `.github/actions/needs_success`.                        |
| `release.yml` | `wasm`, `candid`, `release` | Tag-only — only runs on `push: tags: v*`.                                   |

The `Prepare` composite action (`.github/actions/prepare`) installs
the rust toolchain (`rust-toolchain.toml`), npm deps, `cargo-binstall`,
`cargo-sort`, `cargo-nextest`, `shfmt`, `zizmor`, and `yq`. Touch
[`dev-tools.json`](../../dev-tools.json) when you need to add or
upgrade one of these.

## 7. After CI fails

- **`format` failed and pushed a fixup commit** → pull, you're fine.
- **`format` failed without a PAT** (forks) → run `npm run format`
  locally and push.
- **`lint` failed** → run `npm run lint:rust` (auto-fixes clippy where
  possible). Never silence with `#[allow(clippy::…)]` unless you can
  justify it in code review. Pre-existing
  `pedantic = { level = "deny", priority = -1 }` is intentional.
- **`unit` / `integration` failed** → reproduce locally:
  ```bash
  RUSTFLAGS="-D warnings" cargo test --lib --bins
  RUSTFLAGS="-D warnings" cargo test --test it
  ```
  Never commit `#[ignore]` to bypass — that violates
  [`.policies/testing-policy.md`](../../.policies/testing-policy.md).

## 8. Updating an existing PR

When you need to change a PR after it has been pushed (review feedback,
CI fixes, follow-up tweaks), **add new commits**. Do not rewrite history.

- **Never `git push --force` / `--force-with-lease`** on a PR branch.
- **Never `git commit --amend`** a commit that is already on the remote.
- **Never `git rebase` / squash / reword** pushed commits.
- Just `git commit` the new change and `git push` — the PR will pick it
  up.

The only exception is an **explicit user instruction** to force-push,
amend, rebase, or otherwise rewrite history (e.g. "please squash these
commits and force-push"). Without that explicit command, treat the
branch history as append-only.

## 9. CODEOWNERS auto-routing

[`.github/CODEOWNERS`](../../.github/CODEOWNERS) currently routes every
path to `@antonioventilii`. Agents do not assign reviewers — the file
does it. If/when team-based ownership is introduced (mirror
control-panel for the pattern), update this section in the same PR.

## 10. Release flow (informational)

Releases are triggered by pushing a `v*` tag. `release.yml` builds
WASMs + Candid for `clearing`, `registry`, `minter`, plus the
downloaded `ledger` / `index` artifacts, and attaches them to a
GitHub Release via `softprops/action-gh-release`. There is no
`release-please` automation — version bumps are manual `chore: vX.Y.Z`
commits.

Do not edit release artifacts in tree (`*.wasm.gz`, `*.did`) — they
come from the build, not from source.
