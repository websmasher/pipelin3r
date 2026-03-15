#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
JARPATH="/Users/tartakovsky/Projects/schedulr_old/apps/schedulr/app/target/schedulr-app-0.0.1-SNAPSHOT-exec.jar"
FIXTURES_DIR="$SCRIPT_DIR/fixtures"
GOLDEN_DIR="$SCRIPT_DIR/golden"
REST_PORT=17945

# --- Parse flags ---
RUN_CLI=true
RUN_REST=true
for arg in "$@"; do
  case "$arg" in
    --cli-only) RUN_REST=false ;;
    --rest-only) RUN_CLI=false ;;
    *) echo "Unknown flag: $arg"; exit 1 ;;
  esac
done

# --- Helpers ---

# Filter Spring Boot log lines from output
filter_spring_logs() {
  grep -vE '^[0-9]{4}-[0-9]{2}-[0-9]{2}[ T].*\b(INFO|WARN|ERROR|DEBUG|TRACE)\b' || true
}

# Normalize CLI output: replace Duration values and timestamps with DYNAMIC,
# collapse stack traces to first line + STACKTRACE_OMITTED
normalize_cli() {
  local in_stacktrace=false
  local first_exception_line=""
  while IFS= read -r line; do
    # Filter Spring Boot logs
    if echo "$line" | grep -qE '^[0-9]{4}-[0-9]{2}-[0-9]{2}[ T].*\b(INFO|WARN|ERROR|DEBUG|TRACE)\b'; then
      continue
    fi
    # Replace Duration value with DYNAMIC
    if echo "$line" | grep -qE '^Duration:'; then
      echo "Duration: DYNAMIC"
      continue
    fi
    # Detect start of a Java stack trace (exception line)
    if echo "$line" | grep -qE '^\S+(\.\S+)*(Exception|Error|Throwable)'; then
      if [ "$in_stacktrace" = false ]; then
        in_stacktrace=true
        # Strip Java class prefix, keep only the message
        local msg
        msg=$(echo "$line" | sed 's/^[^:]*: //')
        echo "ERROR: $msg"
      fi
      continue
    fi
    # Skip "at ..." and "Caused by:" lines in stack traces
    if [ "$in_stacktrace" = true ]; then
      if echo "$line" | grep -qE '^\s+at |^Caused by:|^\s+\.\.\. [0-9]+ more'; then
        continue
      else
        in_stacktrace=false
      fi
    fi
    echo "$line"
  done
}

# Normalize REST JSON: replace started_at and elapsed with DYNAMIC
normalize_rest_json() {
  python3 -c "
import sys, json
raw = sys.stdin.read().strip()
if not raw:
    print('{}')
    sys.exit(0)
try:
    obj = json.loads(raw)
except json.JSONDecodeError:
    print(raw)
    sys.exit(0)
def normalize(o):
    if isinstance(o, dict):
        for k in o:
            if k in ('started_at', 'elapsed'):
                o[k] = 'DYNAMIC'
            else:
                normalize(o[k])
    elif isinstance(o, list):
        for item in o:
            normalize(item)
normalize(obj)
print(json.dumps(obj, sort_keys=True, indent=2))
"
}

# Run a CLI test and save golden output
# Usage: run_cli_test <test-name> <extra-args...>
run_cli_test() {
  local test_name="$1"
  shift
  local outfile="$GOLDEN_DIR/cli/${test_name}.txt"

  echo "  CLI: ${test_name}..."

  local exit_code=0
  local raw_output
  raw_output=$(java -jar "$JARPATH" "$@" 2>&1) || exit_code=$?

  local normalized
  normalized=$(echo "$raw_output" | normalize_cli)

  {
    echo "EXIT_CODE: ${exit_code}"
    echo "OUTPUT:"
    echo "$normalized"
  } > "$outfile"
}

# Run a REST test and save golden output
# Usage: run_rest_test <test-name> <curl-args...>
run_rest_test() {
  local test_name="$1"
  shift
  local outfile="$GOLDEN_DIR/rest/${test_name}.json"

  echo "  REST: ${test_name}..."

  local raw_output
  raw_output=$(curl -s "$@" 2>/dev/null) || true

  echo "$raw_output" | normalize_rest_json > "$outfile"
}

# Read a fixture file and return its content as a single string for REST payloads
fixture_content() {
  cat "$FIXTURES_DIR/$1"
}

# --- Setup ---
mkdir -p "$GOLDEN_DIR/cli" "$GOLDEN_DIR/rest"

CLI_COUNT=0
REST_COUNT=0

# --- CLI tests ---
if [ "$RUN_CLI" = true ]; then
  echo ""
  echo "=== Capturing CLI golden tests ==="
  echo ""

  run_cli_test "01-basic-echo" execute -t "$FIXTURES_DIR/01-basic-echo.yaml"
  CLI_COUNT=$((CLI_COUNT + 1))

  run_cli_test "02-run-field" execute -t "$FIXTURES_DIR/02-run-field.yaml"
  CLI_COUNT=$((CLI_COUNT + 1))

  run_cli_test "03-multiline-output" execute -t "$FIXTURES_DIR/03-multiline-output.yaml"
  CLI_COUNT=$((CLI_COUNT + 1))

  run_cli_test "04-exit-nonzero" execute -t "$FIXTURES_DIR/04-exit-nonzero.yaml"
  CLI_COUNT=$((CLI_COUNT + 1))

  run_cli_test "05-exit-code-42" execute -t "$FIXTURES_DIR/05-exit-code-42.yaml"
  CLI_COUNT=$((CLI_COUNT + 1))

  run_cli_test "06-command-not-found" execute -t "$FIXTURES_DIR/06-command-not-found.yaml"
  CLI_COUNT=$((CLI_COUNT + 1))

  run_cli_test "07-empty-output" execute -t "$FIXTURES_DIR/07-empty-output.yaml"
  CLI_COUNT=$((CLI_COUNT + 1))

  run_cli_test "08-stderr-only" execute -t "$FIXTURES_DIR/08-stderr-only.yaml"
  CLI_COUNT=$((CLI_COUNT + 1))

  run_cli_test "09-mixed-streams" execute -t "$FIXTURES_DIR/09-mixed-streams.yaml"
  CLI_COUNT=$((CLI_COUNT + 1))

  run_cli_test "10-special-chars" execute -t "$FIXTURES_DIR/10-special-chars.yaml"
  CLI_COUNT=$((CLI_COUNT + 1))

  run_cli_test "11-timeout-exceeded" execute -t "$FIXTURES_DIR/11-timeout-exceeded.yaml"
  CLI_COUNT=$((CLI_COUNT + 1))

  run_cli_test "12-stdin-input" execute -t "$FIXTURES_DIR/12-stdin-cat.yaml" -i "hello from stdin"
  CLI_COUNT=$((CLI_COUNT + 1))

  run_cli_test "13-stdin-transform" execute -t "$FIXTURES_DIR/13-stdin-transform.yaml" -i "make me uppercase"
  CLI_COUNT=$((CLI_COUNT + 1))

  run_cli_test "14-env-vars" execute -t "$FIXTURES_DIR/14-env-echo.yaml" -e TEST_VAR=golden_value
  CLI_COUNT=$((CLI_COUNT + 1))

  run_cli_test "15-workdir" execute -t "$FIXTURES_DIR/15-workdir-pwd.yaml" -w /tmp
  CLI_COUNT=$((CLI_COUNT + 1))

  run_cli_test "16-pipe-command" execute -t "$FIXTURES_DIR/16-pipe-command.yaml"
  CLI_COUNT=$((CLI_COUNT + 1))

  run_cli_test "17-retry-exhausted" execute -t "$FIXTURES_DIR/17-retry-always-fail.yaml"
  CLI_COUNT=$((CLI_COUNT + 1))

  run_cli_test "21-no-command-error" execute -t "$FIXTURES_DIR/21-no-command.yaml"
  CLI_COUNT=$((CLI_COUNT + 1))

  run_cli_test "22-invalid-yaml-error" execute -t "$FIXTURES_DIR/22-invalid-yaml.yaml"
  CLI_COUNT=$((CLI_COUNT + 1))

  run_cli_test "23-long-output" execute -t "$FIXTURES_DIR/23-long-output.yaml"
  CLI_COUNT=$((CLI_COUNT + 1))

  run_cli_test "inline-yaml" execute -t $'name: inline\ncommand: echo inline works\ntimeout: 10s'
  CLI_COUNT=$((CLI_COUNT + 1))

  run_cli_test "limiter-key-override" execute -t "$FIXTURES_DIR/18-rate-limited.yaml" -k custom-key
  CLI_COUNT=$((CLI_COUNT + 1))

  echo ""
  echo "Captured ${CLI_COUNT} CLI golden tests"
fi

# --- REST tests ---
if [ "$RUN_REST" = true ]; then
  echo ""
  echo "=== Capturing REST golden tests ==="
  echo ""

  # Start the daemon
  echo "  Starting REST daemon on port ${REST_PORT}..."
  env -u CLAUDECODE java -Dserver.port=$REST_PORT -jar "$JARPATH" &>/tmp/schedulr-golden-daemon.log &
  DAEMON_PID=$!
  trap "kill $DAEMON_PID 2>/dev/null; wait $DAEMON_PID 2>/dev/null" EXIT

  # Wait for startup
  for i in $(seq 1 30); do
    if curl -s "http://localhost:${REST_PORT}/api/tasks/status" >/dev/null 2>&1; then
      echo "  Daemon ready (took ${i}s)"
      break
    fi
    if [ "$i" -eq 30 ]; then
      echo "ERROR: Daemon failed to start within 30s"
      echo "Daemon log:"
      cat /tmp/schedulr-golden-daemon.log
      exit 1
    fi
    sleep 1
  done

  BASE="http://localhost:${REST_PORT}"

  # Helper to build JSON payload from fixture file
  rest_payload_from_fixture() {
    local fixture_file="$1"
    shift
    local content
    content=$(cat "$FIXTURES_DIR/$fixture_file")
    # Escape for JSON
    local escaped
    escaped=$(python3 -c "import sys,json; print(json.dumps(sys.stdin.read()))" <<< "$content")
    # Build the JSON object with optional extra fields
    local extra_fields=""
    for field in "$@"; do
      extra_fields="${extra_fields}, $field"
    done
    echo "{\"task\": ${escaped}${extra_fields}}"
  }

  run_rest_test "01-basic-echo" \
    -X POST "$BASE/api/tasks" \
    -H "Content-Type: application/json" \
    -d "$(rest_payload_from_fixture 01-basic-echo.yaml)"
  REST_COUNT=$((REST_COUNT + 1))

  run_rest_test "04-exit-nonzero" \
    -X POST "$BASE/api/tasks" \
    -H "Content-Type: application/json" \
    -d "$(rest_payload_from_fixture 04-exit-nonzero.yaml)"
  REST_COUNT=$((REST_COUNT + 1))

  run_rest_test "11-timeout" \
    -X POST "$BASE/api/tasks" \
    -H "Content-Type: application/json" \
    -d "$(rest_payload_from_fixture 11-timeout-exceeded.yaml)"
  REST_COUNT=$((REST_COUNT + 1))

  run_rest_test "12-stdin" \
    -X POST "$BASE/api/tasks" \
    -H "Content-Type: application/json" \
    -d "$(rest_payload_from_fixture 12-stdin-cat.yaml '"input": "hello from stdin"')"
  REST_COUNT=$((REST_COUNT + 1))

  run_rest_test "14-env" \
    -X POST "$BASE/api/tasks" \
    -H "Content-Type: application/json" \
    -d "$(rest_payload_from_fixture 14-env-echo.yaml '"environment": {"TEST_VAR": "golden_value"}')"
  REST_COUNT=$((REST_COUNT + 1))

  run_rest_test "15-workdir" \
    -X POST "$BASE/api/tasks" \
    -H "Content-Type: application/json" \
    -d "$(rest_payload_from_fixture 15-workdir-pwd.yaml '"working_directory": "/tmp"')"
  REST_COUNT=$((REST_COUNT + 1))

  run_rest_test "21-bad-yaml" \
    -X POST "$BASE/api/tasks" \
    -H "Content-Type: application/json" \
    -d "$(rest_payload_from_fixture 21-no-command.yaml)"
  REST_COUNT=$((REST_COUNT + 1))

  run_rest_test "status-empty" \
    "$BASE/api/tasks/status"
  REST_COUNT=$((REST_COUNT + 1))

  run_rest_test "limiter-status-empty" \
    "$BASE/api/tasks/limiter-status"
  REST_COUNT=$((REST_COUNT + 1))

  # Clean up daemon
  kill $DAEMON_PID 2>/dev/null || true
  wait $DAEMON_PID 2>/dev/null || true
  trap - EXIT

  echo ""
  echo "Captured ${REST_COUNT} REST golden tests"
fi

# --- Summary ---
TOTAL=$((CLI_COUNT + REST_COUNT))
echo ""
echo "==============================="
echo "Captured ${TOTAL} golden tests total (${CLI_COUNT} CLI, ${REST_COUNT} REST)"
echo "Golden files saved to: ${GOLDEN_DIR}/"
echo "==============================="
