# Release & Deployment

This repository uses **release-please** for versioning/releases and a separate
**deploy** workflow for shipping canisters to the IC. The two are decoupled: a
release produces a tag + GitHub Release; deployment reacts to `main` pushes and
tags.

## Flow at a glance

```
feature PR ──merge──▶ main ──▶ deploy.yml ──▶ STAGING auto-deploy (no --yes)
                        │
                        └─▶ release-please.yml  keeps a "chore(release): vX.Y.Z" PR

approve + merge the release PR ──▶ main
                        │
                        ├─▶ release-please.yml  creates tag vX.Y.Z + Release + CHANGELOG
                        │        ├─▶ release.yml   attaches WASMs + candid to the Release
                        │        └─▶ deploy.yml    PRODUCTION deploy (network: ic, no --yes)
                        └─▶ deploy.yml   STAGING re-deploy of the same commit (no-op upgrade)
```

## Workflows

| Workflow             | Trigger                                               | Purpose                                                                               |
| -------------------- | ----------------------------------------------------- | ------------------------------------------------------------------------------------- |
| `release-please.yml` | push to `main`, manual                                | Maintain the release PR; on merge, create tag + Release + notes, bump `package.json`. |
| `release.yml`        | GitHub Release `released`, manual                     | Build `clearing/registry/minter` WASMs + candid and attach as release assets.         |
| `deploy.yml`         | push to `main` (→ staging), `v*` tag (→ prod), manual | `dfx deploy` to the target network, then record the installed module hashes.          |

Conventional Commit PR titles (which feed the changelog) are enforced by the
`pr-title` job in `checks.yml`.

### Versioning

`release-type: simple` — the release version lives in `.release-please-manifest.json`
and `CHANGELOG.md`, and is mirrored into `package.json` via `extra-files`.

The Rust workspace version in `Cargo.toml` is **intentionally not bumped**:
`scripts/build.sh` builds with `cargo build --locked`, and bumping the workspace
version without regenerating `Cargo.lock` would break every locked build. No
canister surfaces `CARGO_PKG_VERSION`, so the crate version is cosmetic. If we
ever want it synced, switch `release-type` to `rust` (it updates `Cargo.toml`
and `Cargo.lock` together).

Pre-1.0 bumping (`bump-minor-pre-major` + `bump-patch-for-minor-pre-major`):
`feat` → patch, breaking (`!`) → minor. This keeps us on `0.x` until we opt into
`1.0.0`.

### Deploy semantics

- Deploys always pass **`--upgrade-unchanged`** so `post_upgrade` always runs
  and no canister is silently skipped by a hash check.
- **Automatic** runs (push to `main`, `v*` tag) omit **`--yes`**.
- **Force tick**: run `deploy.yml` manually (Actions → Deploy → Run workflow),
  choose the environment, and tick **force** to add `--yes` and auto-confirm a
  deploy that would otherwise stop at a confirmation prompt (e.g. after an issue).
- `dfx deploy` runs without a canister list, so it deploys `clearing`, `registry`,
  `minter`, `ledger` and `index`. Only `icp_ledger` is skipped: it is `remote` on
  `staging` and `ic`.

### Installed module hashes

A release `.wasm.gz` never matches the chain, because `dfx deploy` builds its own
module with Candid metadata. So `deploy.yml` reads the hash each canister actually
has right after deploying, with `scripts/record.module-hashes.sh`:

- **Every run** writes canister, canister ID, module hash and git ref to the job
  summary, with the job status. This also runs after a failed deploy, so a row shows
  what is live, which can be older code than the git ref beside it. `None` means no
  module is installed; `unavailable` means the read failed.
- **`v*` tag runs to production** also append that section to the notes of the
  matching GitHub Release, because job summaries and logs expire after about 90 days.
  The `record-release-hashes` job does this separately, so the deploy job keeps
  `contents: read`. A re-run appends another section. If the release still cannot be
  read after about two minutes the job ends with a warning, and the table has to be
  copied from the job summary by hand.

A production deploy dispatched from a branch has no matching release, so only its job
summary records it.

To check for an out-of-band upgrade, read the hash on mainnet, with no controller
rights needed, and compare it with the newest release that has this section:

```bash
dfx --identity anonymous canister --network ic info g5pxl-pyaaa-aaaaj-qqhoq-cai
```

```bash
curl -s https://ic-api.internetcomputer.org/api/v3/canisters/g5pxl-pyaaa-aaaaj-qqhoq-cai | jq -r .module_hash
```

`dfx` prints the hash with a `0x` prefix and the API without one. A hash that appears in
no release was installed by something other than a tag deploy, such as a manual
`dfx deploy`. Canister IDs are in `canister_ids.json`.

## One-time setup

### 1. Release bot (GitHub App)

The release PR is opened by a GitHub App bot, not `GITHUB_TOKEN`, so that (a) a
human can approve it (you can't approve your own PR) and (b) CI runs on the
release PR (`GITHUB_TOKEN`-authored PRs don't trigger workflows).

1. GitHub → Settings → Developer settings → **GitHub Apps → New GitHub App**.
2. Untick **Webhook → Active**.
3. Repository permissions: **Contents: Read & write**, **Pull requests: Read &
   write**, **Issues: Read & write** (for PR labels). Nothing else.
4. Install-on: **Only on this account**. Create.
5. Note the **App ID**; **Generate a private key** (downloads a `.pem`).
6. **Install App** on `icdc-core`.
7. Repo → Settings → Secrets and variables → Actions:
   - Variable **`RELEASE_BOT_APP_ID`** = the App ID
   - Secret **`RELEASE_BOT_PRIVATE_KEY`** = full `.pem` contents

### 2. Deploy identity (controller PEM)

A single, dedicated, revocable CI identity serves both environments. Add it as
an additional controller on both the `staging` and `ic` canisters — do not put
your personal identity in CI.

```bash
dfx identity new icdc-ci-deploy --storage-mode plaintext
CI_PRINCIPAL=$(dfx --identity icdc-ci-deploy identity get-principal)

# Run as the current controller identity, on both networks:
for net in staging ic; do
  for c in clearing registry minter; do
    dfx canister --network "$net" update-settings "$c" --add-controller "$CI_PRINCIPAL"
  done
done

# base64 keeps the multiline PEM intact inside a GitHub secret:
dfx identity export icdc-ci-deploy | base64 | gh secret set DFX_DEPLOY_KEY
```

### 3. Branch protection (recommended)

Protect `main`: require 1 approving review + required status checks
(`Checks Pass`, `Tests Pass`, `PR Title`). Since the bot authors the release PR,
your approval is valid.

## Secrets & variables

| Name                      | Kind     | Used by              | Purpose                                                      |
| ------------------------- | -------- | -------------------- | ------------------------------------------------------------ |
| `RELEASE_BOT_APP_ID`      | variable | `release-please.yml` | GitHub App id for the release bot                            |
| `RELEASE_BOT_PRIVATE_KEY` | secret   | `release-please.yml` | GitHub App private key                                       |
| `DFX_DEPLOY_KEY`          | secret   | `deploy.yml`         | base64 PEM of the CI controller identity (both environments) |
