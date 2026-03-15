# Mutation testing round 1: 53 mutant-killing tests

**Date:** 2026-03-15 19:49
**Scope:** All three packages

## Results
| Crate | Before | After |
|-------|--------|-------|
| limit3r | 57% kill (47 survived) | 82% kill (19 survived) |
| SDK | 65% kill (7 survived) | 90% kill (2 survived) |
| pipelin3r | 78% kill (21 survived) | 92% kill (7 survived) |

53 new tests, 47 mutants killed. 28 remaining survivors mostly in eviction boundary conditions at MAX_TRACKED_KEYS (10K entries).

229 total tests, all passing.
