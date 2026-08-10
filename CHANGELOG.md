# Changelog

## [0.1.8](https://github.com/AntonioVentilii/icdc-core/compare/v0.1.7...v0.1.8) (2026-07-24)


### Features

* **clearing,registry:** batch/filter reads for the resolution solver ([#112](https://github.com/AntonioVentilii/icdc-core/issues/112)) ([5a12d0c](https://github.com/AntonioVentilii/icdc-core/commit/5a12d0cd5d2ff9ee848fcdae46bd0abdf3026a38))


### Bug Fixes

* **clearing:** send explicit fee on ICRC transfers with cached-fee + BadFee retry ([#111](https://github.com/AntonioVentilii/icdc-core/issues/111)) ([7867e15](https://github.com/AntonioVentilii/icdc-core/commit/7867e15e9f052cbf76063e9adff1e259a89db53f))

## [0.1.7](https://github.com/AntonioVentilii/icdc-core/compare/v0.1.6...v0.1.7) (2026-07-22)


### Bug Fixes

* **scripts:** use lowercase echo in ledger/index arg builders ([#107](https://github.com/AntonioVentilii/icdc-core/issues/107)) ([3c4d11d](https://github.com/AntonioVentilii/icdc-core/commit/3c4d11d7fa25bc39109aa6fef672247b2a6cdcd2))

## [0.1.6](https://github.com/AntonioVentilii/icdc-core/compare/v0.1.5...v0.1.6) (2026-07-22)


### Features

* **clearing:** reject new exposure on expired series ([#94](https://github.com/AntonioVentilii/icdc-core/issues/94)) ([6a07969](https://github.com/AntonioVentilii/icdc-core/commit/6a079695a0d3715cf401fa50027b4c5c3b9b2b70))
* **clearing:** reject trades before a series' start_ns ([#93](https://github.com/AntonioVentilii/icdc-core/issues/93)) ([079a86f](https://github.com/AntonioVentilii/icdc-core/commit/079a86fc8b907151dbdf1b40f80c4e489eb0a7ee))
* **shared,registry:** optional start_ns for scheduled series ([#92](https://github.com/AntonioVentilii/icdc-core/issues/92)) ([897c64e](https://github.com/AntonioVentilii/icdc-core/commit/897c64eb5673d44e13ee10985a4ed069a3d744d8))


### Bug Fixes

* **deploy:** deploy via npm scripts in CI ([#100](https://github.com/AntonioVentilii/icdc-core/issues/100)) ([269f7d9](https://github.com/AntonioVentilii/icdc-core/commit/269f7d92759f309693523fe236717bc94f0c806f))


### Continuous Integration

* **checks:** add a scope-required PR title check ([#75](https://github.com/AntonioVentilii/icdc-core/issues/75)) ([c107656](https://github.com/AntonioVentilii/icdc-core/commit/c107656b18a950d0bd82d35659f02daad9451ca2))
* **did:** verify committed Candid (.did) files match generated output ([#95](https://github.com/AntonioVentilii/icdc-core/issues/95)) ([67b37bb](https://github.com/AntonioVentilii/icdc-core/commit/67b37bb17ed50fa852dcddcaa3ce50986f12db87))
* **lint:** cache cargo build for clippy ([#97](https://github.com/AntonioVentilii/icdc-core/issues/97)) ([402103e](https://github.com/AntonioVentilii/icdc-core/commit/402103ec6b361079f52a62af705612f4bc2c7f19))
* **prettier:** ignore release-please generated files ([#99](https://github.com/AntonioVentilii/icdc-core/issues/99)) ([8f981f8](https://github.com/AntonioVentilii/icdc-core/commit/8f981f8251cd501316de362cde88e04d27401a8c))
* **release:** add release-please + auto staging/prod canister deploy ([#96](https://github.com/AntonioVentilii/icdc-core/issues/96)) ([e1579d3](https://github.com/AntonioVentilii/icdc-core/commit/e1579d3cd525a41c97e5e93965c9e0dd25bad062))
* **release:** bump Cargo.toml + Cargo.lock on release ([#101](https://github.com/AntonioVentilii/icdc-core/issues/101)) ([438d3f9](https://github.com/AntonioVentilii/icdc-core/commit/438d3f9f57f76f9c39d182a88afbc800aa21fb76))
* **release:** bump package-lock.json version on release ([#106](https://github.com/AntonioVentilii/icdc-core/issues/106)) ([3c0fb1e](https://github.com/AntonioVentilii/icdc-core/commit/3c0fb1e01c2679a488feca3ecb906644198fe200))


### Miscellaneous Chores

* **cargo-deps:** bump the ic-cdk-kit group with 2 updates ([#89](https://github.com/AntonioVentilii/icdc-core/issues/89)) ([f6216c9](https://github.com/AntonioVentilii/icdc-core/commit/f6216c9c7ef319c0dcbfdda4b9e802e4b1d587a2))
* **github-actions:** bump actions/checkout from 6.0.1 to 7.0.0 ([#90](https://github.com/AntonioVentilii/icdc-core/issues/90)) ([0d99086](https://github.com/AntonioVentilii/icdc-core/commit/0d99086121b3aff7c91ff62577a03e4e2e6dceeb))
* **npm-deps-dev:** bump prettier from 3.8.3 to 3.8.4 ([#88](https://github.com/AntonioVentilii/icdc-core/issues/88)) ([1824554](https://github.com/AntonioVentilii/icdc-core/commit/18245544ab3aca257ade8fa025cafb49b932c42d))
* **rust:** bump rust-toolchain from 1.96.0 to 1.97.1 ([#91](https://github.com/AntonioVentilii/icdc-core/issues/91)) ([ffb15ca](https://github.com/AntonioVentilii/icdc-core/commit/ffb15cac6c237cbb208d95781cc20fb128fe01ca))
