# Pre-install all test infrastructure in Docker image

## Goal

The worker Docker image should have every test framework, plugin, and build tool pre-installed so that library setup only needs `clone` + `install library deps` — no reinstalling test runners.

## Current gaps in Dockerfile

| Language | Has | Missing |
|----------|-----|---------|
| Python | pytest | pytest-httpbin, pytest-mock, pytest-cov, pytest-asyncio, responses, trustme, requests-mock, httpretty |
| Go | go test (built-in) | gotestsum (nice to have, not required) |
| Rust | cargo test (built-in) | nothing |
| PHP | phpunit-11 | pest (nice to have) |
| Ruby | rspec, bundler | minitest (stdlib, already there) |
| JavaScript | jest, mocha, ts-node, typescript | chai, vitest |
| C# | dotnet test (built-in) | nothing |
| Java | **nothing** | **maven, gradle** |
| Elixir | **nothing** | **elixir runtime, mix** |

## Changes

### 1. Dockerfile — expand global test tools (line 60-64)

Add comprehensive Python test plugins:
```
pip3 install --break-system-packages \
    pytest pytest-httpbin pytest-mock pytest-cov pytest-asyncio \
    responses trustme requests-mock httpretty tox
```

Add Maven + Gradle:
```
apt-get install -y maven gradle
```

Add Elixir:
```
apt-get install -y elixir erlang-dev
```

### 2. Pipeline install script — use `--system-site-packages` venvs

Change Python install from:
```
python3 -m venv .venv
```
to:
```
python3 -m venv --system-site-packages .venv
```

This gives each library its own isolated venv that inherits the globally installed pytest + plugins. No reinstalling test tools per library.

Remove the explicit `pip install pytest requests-mock` line from the install script — they're already available globally.

### 3. t3str Python runner — no changes needed

The runner already prefers `.venv/bin/python3` which will correctly use the system-site-packages venv. System pytest and plugins are inherited.

## Libraries to test against (21 total)

- Python: 5 (sectxt, pysecuritytxt, securitytxt, securitytxt-parser, python-securitytxt)
- Rust: 3 (sectxt, security-txt, secmap)
- PHP: 3 (security-txt, phpsecuritytxt, security-txt-parser)
- Ruby: 1 (rb-security-txt)
- JavaScript: 2 (security-txt-node-parser, security.txt-extension)
- Go: 6 (securitytxt, security-txt-parser, go-security-txt x2, diosts, securitytxt-parser)
- C#: 1 (DomainDetective)
- Java: 0, Elixir: 0
