Real writer fixtures copied from `/Users/tartakovsky/Projects/steady-parent/packages/generator/bundles/`.

`explosive-aggression/input/` is the actual assembled bundle shape used by Steady Parent's article writer:
- `prompt.md` is the filled writer prompt sent to the model
- the JSON files and `sources/` directory are the bundle context shipped alongside it

`explosive-aggression/expected/article.mdx` is the example generated output that existed in the source bundle.

The fixture intentionally keeps `expected/article.mdx` outside `input/` so `pipelin3r` tests exercise a pre-write workspace rather than a post-write one.
