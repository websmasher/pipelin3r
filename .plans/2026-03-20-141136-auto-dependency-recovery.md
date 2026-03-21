# Auto-Install Missing System Dependencies

## Goal
When a library's build/install fails due to missing system packages, automatically detect what's needed, install it, and retry. This must work across all 9 languages at scale (100+ parsers).

## Approach

The fix goes into `build_install_script()` in `config.rs`. Each language's install script gets a retry loop that:

1. Runs the build/install command, capturing stderr
2. If it fails, scans stderr for known patterns that indicate missing system deps
3. Installs the detected packages via `apt-get install -y`
4. Retries the build

### Known error patterns → apt packages

**Rust** (`cargo build` errors):
- `pkg-config ... nettle` → `nettle-dev`
- `pkg-config ... openssl` → `libssl-dev` (already in Dockerfile)
- `could not find ... clang` / `libclang` → `clang llvm libclang-dev`
- `pkg-config ... glib-2.0` → `libglib2.0-dev`
- `failed to run custom build command` + `pkg-config` → parse the library name from pkg-config output
- Generic: `pkg-config --libs --cflags <LIB>` → `lib<LIB>-dev`

**Python** (`pip install` errors):
- `fatal error: Python.h` → `python3-dev` (already in Dockerfile)
- `fatal error: ffi.h` → `libffi-dev`
- `fatal error: xml/parser.h` → `libexpat1-dev`
- `fatal error: lxml` errors → `libxml2-dev libxslt1-dev` (already in Dockerfile)

**Ruby** (`bundle install` / `gem install` errors):
- `extconf.rb ... sqlite3.h` → `libsqlite3-dev`
- `extconf.rb ... mysql` → `libmysqlclient-dev`
- `extconf.rb ... pg_config` → `libpq-dev`

**C/general**:
- `fatal error: <header>.h: No such file` → search for the package providing it

### Implementation

Rather than hardcoding every possible mapping, use a general strategy:

1. **pkg-config failures**: Extract library name from `pkg-config --libs --cflags <NAME>`, try `apt-get install -y lib<NAME>-dev`
2. **Missing header files**: Extract `<name>.h` from `fatal error: <name>.h`, try `apt-file search <name>.h` to find the package (requires `apt-file` to be installed)
3. **Known mappings**: A small lookup table for common ones that don't follow conventions

### Where to implement

The cleanest approach: add a wrapper function `build_install_with_recovery()` that wraps `build_install_script()` output in a retry-on-missing-deps loop. This is pure shell script — the recovery logic runs on the worker, not in the pipeline Rust code.

## Files to modify

1. `websmasher/tools/dev-process-v3/src/config.rs` — wrap install scripts with dependency recovery loop
