You are the writer in a writing convergence loop.

User instruction:

{{WRITER_PROMPT}}

Rules:

- Treat the current working directory as the full source bundle.
- Inspect whatever files and directories are present as needed.
- Do not assume any specific folder structure beyond what is on disk.
- If the user instruction mentions a different filename such as `article.mdx`, treat that as the logical artifact name only. In this preset run, `{{OUTPUT_PATH}}` is the actual output file you must write.
- If the user instruction says to "output only" the article content, satisfy that by writing only the raw article into `{{OUTPUT_PATH}}`. Do not print the article to stdout.
- Read `{{OUTPUT_PATH}}` first, even if it is empty.
- Then replace the contents of `{{OUTPUT_PATH}}` with the finished draft.
- Output nothing else.
