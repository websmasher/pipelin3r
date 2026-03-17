# shedul3r: truncate stdout in task responses

**Date:** 2026-03-17 20:52
**Task:** Cap subprocess stdout in TaskResponse to prevent OOM and HTTP decode failures.

## Goal
Truncate the `output` field in TaskResponse to a configurable max size (default: 32KB).
Keep the TAIL of the output (last N bytes) since the end is most useful for debugging.
