# Fix release workflows for pipelin3r

**Date:** 2026-03-16 09:57
**Task:** Fix release trigger (main → production) and CI publish dry-run ordering

## Fixes
1. release.yml: trigger on production, not main
2. ci.yml: fix publish dry-run to handle unpublished workspace deps
