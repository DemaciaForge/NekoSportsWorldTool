# PR Integration Report — 2026-10-23

## Summary

- Upstream: `YanamiNeko/NekoSportsWorldTool`, baseline `origin/main` = `e229abdbab71e377b86632e588e607413af4b93c` (2026-10-20, PR #88).
- Fork at audit: `DemaciaForge/NekoSportsWorldTool` `main` = `9ac21e4cddb2a3b71ee7dad17bc58e47af430b4e`; it is divergent and has fork-only history. No default-branch update was attempted.
- Audited the 16 requested PRs against current API state and fetched heads. Integrated #43 and #52 only (2); no others integrated (14). Those two are workflow/UI changes. No executable release is in scope or published by this audit.
- Branch: `integration/upstream-main-pr-audit-2026-10-23`; integrated HEAD recorded after report commit.
- Safety scope: do not include changes enabling fabricated activity records or circumvention of required checkpoints, geofences, attendance, or authenticity checks. No real service requests were made.

## PR decisions

| PR | Current state / head | Review and decision | Validation |
|---|---|---|---|
| #41 | OPEN, `3a725bfd` | Defer/reject for this integration: broad track generation, metric, policy, submit, CLI and UI changes. Includes record-generation/submission behavior and overlaps #60/#106. | Diff inspected; no integration. |
| #42 | OPEN, `c7b3a17c` | Defer: optional fence metadata changes submission wire behavior and overlaps #53. Requires protocol compatibility evidence; not necessary for maintenance-only scope. | Diff inspected; no integration. |
| #43 | OPEN, `29e7fbe7` | Adopt: separates Windows tests, Android smoke checks, and constrains automated Release creation to upstream tag pushes. | Integrated, ancestry verified. Full tests below fail on UI layout tests. |
| #44 | OPEN, `cd05b7ba` | Reject for this integration: adds athletics route mode and generated track behavior, within prohibited activity-record generation scope. | Diff inspected; no integration. |
| #51 | OPEN, `41694e3d` | Reject for this integration: forces required and ordinary checkpoints through generated road routes and changes submission markers; directly intersects required checkpoint/authenticity controls. | Diff inspected; no integration. |
| #52 | OPEN, `ba507ddf` | Adopt: responsive open-source/free-distribution watermark in a reserved UI panel; no activity protocol changes. | Integrated, ancestry verified; full suite has existing layout failures. |
| #53 | OPEN, `4b462edd` | Reject for this integration: changes fence, point selection, policy and route generation/submission paths; directly touches geofence behavior. | Diff inspected; no integration. |
| #57 | OPEN, `4f4fcc98` | Defer/reject: large cross-platform location, login archive, online road and route/UI change set. Login backup expands sensitive-data handling; route and location behavior needs separate privacy and authenticity review. | Diff/path inventory inspected; no integration. |
| #60 | OPEN, `299b5b94` | Reject for this integration: generated road validity distance/time and checkpoint completion wire semantics alter submitted activity data. | Diff inspected; no integration. |
| #63 | OPEN, `2544735b` | Reject for this integration: hand-drawn/imported/custom routes feed generated run tracks; directly within prohibited synthetic activity scope. | Diff/path inventory inspected; no integration. |
| #66 | OPEN, `d3858166` | Defer: only route-planner road graph loader, adding athletics tracks and connecting components. This is route generation infrastructure; no safe standalone non-activity portion established. | Diff inspected; no integration. |
| #79 | OPEN, `02df1401` | Defer: broad Windows/Android UI redesign plus vendored winit platform patch and large UI rewrites; too broad for the maintenance-only integration and needs platform-specific regression review. | Diff/path inventory inspected; no integration. |
| #82 | OPEN, `ac982859` | Defer: Beijing-time formatting affects geometry, ranking and wire timestamps; overlaps track window behavior in #41/#60/#106. Needs focused timestamp compatibility tests. | Diff inspected; no integration. |
| #105 | OPEN, `b0043693` | Reject for this integration: free-run mode bypasses configured checkpoints and adds a distinct submit path. Core feature falls within prohibited authenticity/checkpoint circumvention scope. | Diff inspected; no integration. |
| #106 | OPEN, `1d8d3921` | Reject for this integration: checkpoint coordinates and 10-second window statistics change generated track/checkpoint submission behavior. | Diff inspected; no integration. |
| #107 | OPEN, `a16d1f8f` | Reject for this integration: despite being test-only, it asserts ordinary checkpoint pass counting, directly validating the disallowed checkpoint/submission behavior. | Diff inspected; no integration. |

All requested PR heads currently target `main`; GitHub reported #41, #44, #51, #53, #60, #63, and #82 conflicting when queried. #43/#52 and several others were reported mergeable, which was not treated as correctness evidence. GitHub CI/review data was empty for most PRs at query time; only #43 showed a successful “Full Cargo test suite” status. Current head SHAs are in the table and can be rechecked at the linked PR pages.

## Dependencies and conflicts

- #41/#60/#106 all overlap track metrics, valid distance/time, checkpoint windows and submit flow; do not combine without resolving a single explicit data contract.
- #42/#53 overlap fence metadata and fallback behavior; optional/missing/empty fence states need distinct protocol fixtures.
- #44/#57/#63/#66/#79 overlap routes, map/UI, track mode and platform code; #63/#66 add route-generation inputs and #57/#79 are broad cross-platform changes.
- #51/#60/#107 overlap checkpoint classification, route selection and pass markers. This audit excludes all three from the integration.
- #43/#52 are independent: workflow/release governance and reserved-panel UI. They merged without textual conflicts.
- #82 overlaps timestamp and route-window semantics with #41/#60/#106.
- #105 introduces a separate free-run mode and shares submit/track/UI paths with #41/#51/#60/#106/#107.

Suggested maintainer review order for safe maintenance: #43, then #52. Review #57 login archive/privacy and location components separately from any route-submission features. Keep route generation and activity submission PRs out of a safe public binary until their behavior meets the project’s authenticity requirements. No PR order can make prohibited behavior acceptable.

## Integration and test evidence

- Baseline `e229abdbab71e377b86632e588e607413af4b93c`; branch created cleanly from refreshed upstream main.
- Merged #43 (`29e7fbe7580fcf740a4d708ab5a57cdab1a6d62c`) and #52 (`ba507ddf4f01af66fa290ad041c4e28baab2711b`) with merge commits; both PR heads are ancestors of the branch.
- `git diff --check`: passed.
- `cargo fmt --check`: failed on widespread formatting differences already present in upstream baseline; no mass-formatting applied.
- `cargo test --workspace --locked --offline`: failed, 173 passed, 10 failed, 0 ignored. Failures are UI layout reachability assertions (`outer scroll cannot reach ...`) across phone/desktop cases. One linker warning was emitted. Because the same run included new watermark UI and no clean-baseline comparison was run, attribution to upstream vs #52 is unresolved; this is a release blocker.
- Not run: all-target check, Clippy, Android build/emulator smoke test, external CI on this integration branch, or live-service validation. No service was contacted.
- Fork release inventory included v0.4.1 and older fork releases. No new tag or release created.

## Remote publication and remaining work

Fork `main` is divergent from upstream and was not modified. This audit branch was prepared for publication to the fork; verify the final remote ref and SHA before sharing. No binary Release was created because the full suite failed and the requested activity authenticity boundary excludes the route/submission PRs. No upstream Issue was created in this audit; an issue draft follows.

Remaining risks: determine whether UI layout failures reproduce on untouched upstream baseline; resolve them before treating #52 as validated. Run Android smoke and `cargo check --all-targets --locked` in suitable CI. Re-review current PR heads, CI and review threads before any later action. Review Fork v0.4.1 contents independently; its existence does not establish that all claimed PRs are safe or integrated.

## Draft upstream issue

**Title:** [Integration Report] Review of 16 open PRs against current main

**Body:**

Community review of the 16 PRs listed in the October 23, 2026 audit request, against upstream `main` at `e229abdbab71e377b86632e588e607413af4b93c`. This is an independent, non-official audit in a personal fork; it does not represent upstream acceptance or endorsement.

The audit integrated only maintenance/UI PRs #43 and #52 on branch `integration/upstream-main-pr-audit-2026-10-23`. The full offline workspace test run reported 173 passed and 10 failed, all UI layout reachability assertions; `cargo fmt --check` also reports pre-existing formatting differences. Android smoke and all-target check remain unrun. No release was created. The fork branch/report URL should be added after push verification.

Several PRs change generated route, checkpoint, geofence, or submitted activity semantics (#41, #42, #44, #51, #53, #60, #63, #66, #105, #106, #107). They were not included in this maintenance integration; this is a scope/safety decision, not a claim that each PR is technically incorrect. #57 and #79 need separate privacy/platform compatibility review; #82 needs timestamp compatibility coverage. The report documents per-PR heads, paths, decisions, overlap groups, and actual test results.

Suggested safe review order: #43, #52. Please review route/submission changes separately against the project’s activity authenticity requirements. PR URLs: https://github.com/YanamiNeko/NekoSportsWorldTool/pulls?q=is%3Apr+is%3Aopen+41%2C42%2C43%2C44%2C51%2C52%2C53%2C57%2C60%2C63%2C66%2C79%2C82%2C105%2C106%2C107
