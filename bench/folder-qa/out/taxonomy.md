## Task taxonomy & measured difficulty

_64 cases · 10 models each side · difficulty = mean accuracy across models (measured, not assigned)._


### By life-data domain

| domain | cases | bare | fella | Δ |
|---|--:|--:|--:|--:|
| reading | 11 | 94% | 89% | -6 |
| fitness | 10 | 48% | 95% | +47 |
| subscriptions | 4 | 0% | 95% | +95 |
| contacts | 5 | 96% | 96% | +0 |
| travel | 5 | 96% | 96% | +0 |
| sleep | 3 | 40% | 97% | +57 |
| spending | 12 | 56% | 98% | +42 |
| screen-time | 4 | 30% | 98% | +68 |
| goals | 1 | 100% | 100% | +0 |
| journal | 4 | 100% | 100% | +0 |
| general | 1 | 100% | 100% | +0 |
| housing | 4 | 92% | 100% | +8 |

### By task shape (`tier`)

| tier | cases | bare | fella | Δ |
|---|--:|--:|--:|--:|
| refusal | 1 | 80% | 50% | -30 |
| bool-filter | 1 | 90% | 90% | +0 |
| text-list | 1 | 100% | 90% | -10 |
| text-min | 1 | 0% | 90% | +90 |
| distinct-count | 4 | 92% | 92% | +0 |
| text-max | 3 | 100% | 93% | -7 |
| num-aggregate | 6 | 43% | 93% | +50 |
| num-avg | 5 | 48% | 94% | +46 |
| cat-filter | 2 | 100% | 95% | -5 |
| mf-join-compare | 2 | 60% | 95% | +35 |
| cat-groupby | 5 | 48% | 96% | +48 |
| num-filter | 9 | 52% | 97% | +44 |
| mf-join-aggregate | 3 | 60% | 97% | +37 |
| num-multistep | 2 | 40% | 100% | +60 |
| temporal-max | 1 | 70% | 100% | +30 |
| cat-rank | 2 | 45% | 100% | +55 |
| num-max | 1 | 90% | 100% | +10 |
| text-lookup | 5 | 100% | 100% | +0 |
| text-count | 2 | 100% | 100% | +0 |
| text-search | 1 | 100% | 100% | +0 |
| mf-doc-join | 4 | 70% | 100% | +30 |
| no-tool | 1 | 100% | 100% | +0 |
| mf-join-filter | 1 | 100% | 100% | +0 |
| mf-join-groupby | 1 | 100% | 100% | +0 |

### Structural cuts

| cut | cases | bare | fella | Δ |
|---|--:|--:|--:|--:|
| single-file | 53 | 68% | 95% | +28 |
| multi-file | 11 | 71% | 98% | +27 |
| clean folder | 53 | 69% | 96% | +27 |
| cluttered folder | 11 | 66% | 96% | +30 |
| plain csv/txt only | 41 | 74% | 95% | +21 |
| uses tsv/json/jsonl/md/xlsx/pdf | 23 | 58% | 97% | +40 |
| touches xlsx or pdf | 8 | 46% | 98% | +51 |

### Hardest cases (fella acc < 90%, 2 of 64)

| case | domain | tier | bare | fella | question |
|---|---|---|--:|--:|---|
| fqa-refusal | reading | refusal | 80% | 50% | Based on my reading log, how many books will I finish next year? |
| fqa-read-top-genre-pages | reading | cat-groupby | 90% | 80% | Across the books I finished, which genre did I read the most pages of? |

_18 cases are saturated (bare 100% and fella 100%) — retire or harden these in the next battery._

