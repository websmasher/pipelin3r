# Rewrite steps 4-8 for remote execution via run_remote_command

**Date:** 2026-03-19 12:39
**Task:** All script steps run on shedul3r via run_remote_command. Nothing local.

## Steps to rewrite

| Step | Current | New |
|------|---------|-----|
| 4 | Local git clone + local extract-tools + local pip install + LLM wrappers | Remote: clone + install + extract all via run_remote_command. LLM wrapper gen stays as VerifiedStep. |
| 5 | Local verify | Remote: same verify but via remote command |
| 7 | Local fixture extraction from test JSON | Remote: run actual wrapper scripts on fixtures |
| 8 | Local classification | Can stay local — it's just reading JSON files downloaded from step 7 |

## Key: persistent work directory on Railway

All steps share a persistent directory on Railway (e.g., `/data/pipeline/{package}/`). Step 4 clones repos there, step 7 finds them there. The work_dir in RemoteCommandConfig points to this persistent path.

But wait — we need to know what path to use on Railway. The volume mount path needs to be configured. For now, use a convention: `/data/pipelin3r/{package}/`.

## Files to modify
- s04_clone_and_extract.rs — full rewrite
- s05_extract_source.rs — verify remotely
- s07_run_parsers.rs — full rewrite
- s08_classify.rs — can stay local, reads downloaded results
- config.rs — add remote_work_dir helper
