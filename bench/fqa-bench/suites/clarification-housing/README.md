# Clarification episode: housing scope

This controlled synthetic episode tests one end-to-end behavior: detect that
“housing spending” has no user-defined category mapping, ask whether utilities
and repairs count, then use the user's answer to calculate a Q1 total.

The categories and amounts are deliberately small enough for independent
manual verification. The model receives the complete two-file workspace.
The expected clarification response and final answer are in the evaluator-only
answer key and are not mounted with the workspace.

## Oracle

The user's reply includes rent and utilities and excludes repairs. Q1 2024
therefore totals 1,450 + 95 + 1,450 + 92 + 1,450 + 110 = 4,647 USD.

This is a protocol fixture, not a representative measure of clarification
quality or household accounting behavior. Human review is still required
before the episode is promoted to a scored release set.
