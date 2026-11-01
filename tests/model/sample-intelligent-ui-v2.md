# Sample intelligent UI v2 — frozen real-engine evaluation

Version: sample-intelligent-ui-v2.0.0. Author before the first candidate. The
runner pins SHA-256 of this document and the companion criteria JSON. Any
subsequent correction requires an explicitly approved new version and new run;
preserve all previous outputs. Never tune an expectation to observed output.

Existing convention: provider-backed runners live in tests/evals (see
 tests/README.md). This standalone native-engine evaluation uses the explicitly
requested tests/model path. It does not modify existing tests or runtime code.

## Execute

```sh
node tests/model/sample-intelligent-ui-v2.mjs --list
FELLA_SAMPLE_WORKSPACE='/mnt/c/Users/aviji/Documents/Fella Analyst Sample Workspace' \
FELLA_ENGINE_PATH=backend/target/debug/fella \
node tests/model/sample-intelligent-ui-v2.mjs --live \
  --case monthly-2025 --case regional-explore \
  --case followup-visual --case followup-exact
```

`--all` also includes plain-q4. `--case` may repeat or contain comma-separated
IDs. Selecting a follow-up automatically runs a fresh regional prerequisite;
when both follow-ups are selected they edit successive revisions of the same
regional artifact, visual then exact. All fresh cases have distinct profiles and
conversations. No SQL, fixture result, authoring system prompt, or provider call
is supplied by this harness. Natural user questions are frozen in criteria JSON.
Analysis actually runs native stdio `ask`, using gemma4:31b on Ollama Cloud.
Follow-ups call native `artifact_presentation_edit` with routeFirst:true, matching
the shell's saved-JSON edit path. They are not a claim that arbitrary conversational
`ask` follow-up routing has been tested.

Optional env: FELLA_SAMPLE_AUTH_FILE (default local dev.fella.app/auth.json),
FELLA_SAMPLE_EVAL_OUTPUT (new directory under /tmp), FELLA_SAMPLE_TURN_MS
(1..240000), FELLA_CONTRACT_ACCEPT_PATH (prebuilt native shared validator).
The runner never builds artifacts, invokes a model without --live, uses live app
DB state, or copies the live DB. It reads local auth once, copies only the Ollama
Cloud credential to a private temporary profile, redacts credentials from logs,
and removes each copied auth file on shutdown. No source files are modified.
Reports, profiles without auth, stream events, requests, responses, canonical
runs, evidence, specs and timings remain in /tmp. No automatic reruns.

## Frozen criteria and grading

Each assertion is pass, fail, or unresolved. Correctness and presentation have
separate results. Failed prerequisites produce blocked follow-ups, never passes.
Unresolved semantic/adjudication checks prohibit a claim of a fully passing
case. Infrastructure/setup/timeout failures remain distinct from product grades.
No overall percentage blends these categories. Save selected cases and exclusions.

1. **plain-q4:** fresh native ask. Correctness: saved scalar order count = 102,
   and answer says 102. Presentation: no non-null presentation document; a plain
   answer is appropriate. No canned SQL is injected.
2. **monthly-2025:** fresh computation, not a seeded report/fixture. A complete
   saved result has exactly one row per 2025 month and all 12 audited current-policy
   net amounts. USD values must equal audited cents; tolerance 0.000001 USD only
   accommodates binary serialization, never a one-cent discrepancy. Saved or
   explicitly stated Q3 45187.61, Q4 46139.54 and increase 951.93 are checked
   independently. Quarter counts are 102 each, year 408; monthly counts 34.
   Net/order Q3 443.02, Q4 452.35; after-discount before-refund merchandise AOV
   Q3 465.10, Q4 477.76. These definitions differ; report the denominator.
   Automated checks cover saved monthly net and quarter/delta claims. Counts,
   AOV, metric definitions, policy reading, actual query dedup and period basis
   require explicit source/SQL adjudication; do not infer them solely from a
   correct number. Record those semantic checks unresolved until reviewed.
   Presentation separately requires accepted saved-result-bound monthly chart,
   temporal/month ordering, and an exact-value Table using saved rows. No
   acceptance score substitutes for numerical correctness or readability.
3. **regional-explore:** fresh ask has complete saved monthly regional values
   supporting all actual source regions East/North/South/West (typically 48 rows;
   an additional unallocated refund bucket is allowed and must not become an
   invented geographic region). A reachable supported Select binds declared
   viewer-persisted enum state; domain matches saved geographic regions, with
   optional $all. A reachable Table/Chart consumes that same state with a filter
   on the exact saved region column. Every geographic selection resolves a
   nonempty proper subset of saved rows; this is reference row resolution, not
   a browser click test. Shared native acceptance must pass. No invented regional
   or category refund-allocation golden values.
4. **followup-visual / followup-exact:** edit the current saved regional JSON
   through the native tool-free editor and real model. Require accepted changed
   JSON, same artifact and canonical run, revision +1, identical saved results,
   unchanged canonical analytical record and evidence, identical analytical-run
   file set, no analytical tool/evidence events. Retain region state key,
   declaration, options and viewer persistence, with an actual connected consumer.
   Visual requires a bound monthly line chart; exact requires a bound Table.
   The exact follow-up follows the visual revision when both selected. Display
   state is viewer-local: retained schema permits restoration; this native-only
   harness does not establish live browser state persistence or event wiring.
5. **repair:** malformed-output repair is excluded from live model-quality
   cases. Deterministic coverage remains separate, e.g.
   `cargo test --manifest-path backend/Cargo.toml --lib engine::presentation_edit::tests`
   and `tests/unit/javascript/test-intelligent-ui-contract.mjs`. The runner does
   not inject malformed provider replies, run those suites, or change them.

## Policy, ambiguity and audit basis

Authority: docs/metric_policy_current.md. Sum quantity × transaction line price
minus line discount, exclude cancelled orders, subtract posted refunds by refund
transaction date, exclude pending refunds, tax and shipping. Retain pending
orders: policy does not exclude them. Dedup business-identical overlapping annual
order IDs, unique line IDs/refund IDs, and product category lookup grain. Never
join raw products or raw refund events onto lines and repeat values. Product P001
has conflicting names/active flags; do not invent a canonical metadata winner.
Category net after order-level refunds is not identifiable without an allocation
assumption; no category-net gold is manufactured here.

Literal all-posted policy includes unmatched posted refund RF-ORPHAN-01, 64.25 in
December: Q4 46139.54. Its business validity is uncertain. Matched-order-only Q4
46203.79 is a sensitivity scope, acceptable to disclose but it does NOT silently
replace the frozen literal-policy numerical check. Pending June refund 144 is
excluded. Source-owner ambiguity is recorded separately from deterministic totals.
Q3 45714.32 double-counts September's 526.71 overlap; dedup yields 45187.61.

The criteria JSON freezes all 12 monthly amounts and audited source hashes.
The runner checks the audited source snapshot before calls and source hashes
again after calls. Source drift blocks comparison; no automatic gold regeneration.
It records git HEAD/dirty paths, model, binary and catalog hashes, questions,
criteria hashes, native trace/model-call metadata, requests/events/responses,
complete evidence, timings and unchanged-state comparisons. Provider token use
is recorded where returned; edit API exposes no token usage, reported unavailable.
No model judges or manual scores are silently synthesized.
