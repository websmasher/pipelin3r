#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
FIXTURES_DIR="$SCRIPT_DIR/fixtures"
GOLDEN_DIR="$SCRIPT_DIR/golden"
REST_PORT=17945
PASS_COUNT=0
FAIL_COUNT=0

# --- Parse args ---
if [ $# -lt 1 ]; then
  echo "Usage: $0 <binary-path> [--cli-only] [--rest-only]"
  exit 1
fi

BINARY="$1"
shift

RUN_CLI=true
RUN_REST=true
for arg in "$@"; do
  case "$arg" in
    --cli-only) RUN_REST=false ;;
    --rest-only) RUN_CLI=false ;;
    *) echo "Unknown flag: $arg"; exit 1 ;;
  esac
done

if [ ! -x "$BINARY" ]; then
  echo "ERROR: Binary not found or not executable: $BINARY"
  exit 1
fi

# --- Helpers (same normalization as run-golden.sh) ---

normalize_cli() {
  local in_stacktrace=false
  while IFS= read -r line; do
    if echo "$line" | grep -qE '^[0-9]{4}-[0-9]{2}-[0-9]{2}[ T].*\b(INFO|WARN|ERROR|DEBUG|TRACE)\b'; then
      continue
    fi
    if echo "$line" | grep -qE '^Duration:'; then
      echo "Duration: DYNAMIC"
      continue
    fi
    if echo "$line" | grep -qE '^\S+(\.\S+)*(Exception|Error|Throwable)'; then
      if [ "$in_stacktrace" = false ]; then
        in_stacktrace=true
        local msg
        msg=$(echo "$line" | sed 's/^[^:]*: //')
        echo "ERROR: $msg"
      fi
      continue
    fi
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

# Run a CLI test against the new binary and compare to golden
# Usage: compare_cli_test <test-name> <extra-args...>
compare_cli_test() {
  local test_name="$1"
  shift
  local golden_file="$GOLDEN_DIR/cli/${test_name}.txt"

  if [ ! -f "$golden_file" ]; then
    echo "  SKIP: ${test_name} (no golden file)"
    return
  fi

  local exit_code=0
  local raw_output
  raw_output=$("$BINARY" "$@" 2>&1) || exit_code=$?

  local normalized
  normalized=$(echo "$raw_output" | normalize_cli)

  local actual
  actual=$(printf "EXIT_CODE: %d\nOUTPUT:\n%s\n" "$exit_code" "$normalized")

  local golden
  golden=$(cat "$golden_file")

  if [ "$actual" = "$golden" ]; then
    echo "  PASS: ${test_name}"
    PASS_COUNT=$((PASS_COUNT + 1))
  else
    echo "  FAIL: ${test_name}"
    FAIL_COUNT=$((FAIL_COUNT + 1))
    echo "    --- diff (golden vs actual) ---"
    diff <(echo "$golden") <(echo "$actual") | head -30 || true
    echo "    ---"
  fi
}

# Run a REST test and compare to golden
# Usage: compare_rest_test <test-name> <curl-args...>
compare_rest_test() {
  local test_name="$1"
  shift
  local golden_file="$GOLDEN_DIR/rest/${test_name}.json"

  if [ ! -f "$golden_file" ]; then
    echo "  SKIP: ${test_name} (no golden file)"
    return
  fi

  local raw_output
  raw_output=$(curl -s "$@" 2>/dev/null) || true

  local actual
  actual=$(echo "$raw_output" | normalize_rest_json)

  local golden
  golden=$(cat "$golden_file")

  if [ "$actual" = "$golden" ]; then
    echo "  PASS: ${test_name}"
    PASS_COUNT=$((PASS_COUNT + 1))
  else
    echo "  FAIL: ${test_name}"
    FAIL_COUNT=$((FAIL_COUNT + 1))
    echo "    --- diff (golden vs actual) ---"
    diff <(echo "$golden") <(echo "$actual") | head -30 || true
    echo "    ---"
  fi
}

# Helper to build JSON payload from fixture file
rest_payload_from_fixture() {
  local fixture_file="$1"
  shift
  local content
  content=$(cat "$FIXTURES_DIR/$fixture_file")
  local escaped
  escaped=$(python3 -c "import sys,json; print(json.dumps(sys.stdin.read()))" <<< "$content")
  local extra_fields=""
  for field in "$@"; do
    extra_fields="${extra_fields}, $field"
  done
  echo "{\"task\": ${escaped}${extra_fields}}"
}

# --- CLI tests ---
if [ "$RUN_CLI" = true ]; then
  echo ""
  echo "=== Comparing CLI tests ==="
  echo ""

  compare_cli_test "01-basic-echo" execute -t "$FIXTURES_DIR/01-basic-echo.yaml"
  compare_cli_test "02-run-field" execute -t "$FIXTURES_DIR/02-run-field.yaml"
  compare_cli_test "03-multiline-output" execute -t "$FIXTURES_DIR/03-multiline-output.yaml"
  compare_cli_test "04-exit-nonzero" execute -t "$FIXTURES_DIR/04-exit-nonzero.yaml"
  compare_cli_test "05-exit-code-42" execute -t "$FIXTURES_DIR/05-exit-code-42.yaml"
  compare_cli_test "06-command-not-found" execute -t "$FIXTURES_DIR/06-command-not-found.yaml"
  compare_cli_test "07-empty-output" execute -t "$FIXTURES_DIR/07-empty-output.yaml"
  compare_cli_test "08-stderr-only" execute -t "$FIXTURES_DIR/08-stderr-only.yaml"
  compare_cli_test "09-mixed-streams" execute -t "$FIXTURES_DIR/09-mixed-streams.yaml"
  compare_cli_test "10-special-chars" execute -t "$FIXTURES_DIR/10-special-chars.yaml"
  compare_cli_test "11-timeout-exceeded" execute -t "$FIXTURES_DIR/11-timeout-exceeded.yaml"
  compare_cli_test "12-stdin-input" execute -t "$FIXTURES_DIR/12-stdin-cat.yaml" -i "hello from stdin"
  compare_cli_test "13-stdin-transform" execute -t "$FIXTURES_DIR/13-stdin-transform.yaml" -i "make me uppercase"
  compare_cli_test "14-env-vars" execute -t "$FIXTURES_DIR/14-env-echo.yaml" -e TEST_VAR=golden_value
  compare_cli_test "15-workdir" execute -t "$FIXTURES_DIR/15-workdir-pwd.yaml" -w /tmp
  compare_cli_test "16-pipe-command" execute -t "$FIXTURES_DIR/16-pipe-command.yaml"
  compare_cli_test "17-retry-exhausted" execute -t "$FIXTURES_DIR/17-retry-always-fail.yaml"
  compare_cli_test "21-no-command-error" execute -t "$FIXTURES_DIR/21-no-command.yaml"
  compare_cli_test "22-invalid-yaml-error" execute -t "$FIXTURES_DIR/22-invalid-yaml.yaml"
  compare_cli_test "23-long-output" execute -t "$FIXTURES_DIR/23-long-output.yaml"
  compare_cli_test "inline-yaml" execute -t $'name: inline\ncommand: echo inline works\ntimeout: 10s'
  compare_cli_test "limiter-key-override" execute -t "$FIXTURES_DIR/18-rate-limited.yaml" -k custom-key
fi

# --- REST tests ---
if [ "$RUN_REST" = true ]; then
  echo ""
  echo "=== Comparing REST tests ==="
  echo ""

  # Start the new binary as a REST daemon
  echo "  Starting REST daemon on port ${REST_PORT}..."
  "$BINARY" --port "$REST_PORT" &>/tmp/schedulr-compare-daemon.log &
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
      cat /tmp/schedulr-compare-daemon.log
      exit 1
    fi
    sleep 1
  done

  BASE="http://localhost:${REST_PORT}"

  compare_rest_test "01-basic-echo" \
    -X POST "$BASE/api/tasks" \
    -H "Content-Type: application/json" \
    -d "$(rest_payload_from_fixture 01-basic-echo.yaml)"

  compare_rest_test "04-exit-nonzero" \
    -X POST "$BASE/api/tasks" \
    -H "Content-Type: application/json" \
    -d "$(rest_payload_from_fixture 04-exit-nonzero.yaml)"

  compare_rest_test "11-timeout" \
    -X POST "$BASE/api/tasks" \
    -H "Content-Type: application/json" \
    -d "$(rest_payload_from_fixture 11-timeout-exceeded.yaml)"

  compare_rest_test "12-stdin" \
    -X POST "$BASE/api/tasks" \
    -H "Content-Type: application/json" \
    -d "$(rest_payload_from_fixture 12-stdin-cat.yaml '"input": "hello from stdin"')"

  compare_rest_test "14-env" \
    -X POST "$BASE/api/tasks" \
    -H "Content-Type: application/json" \
    -d "$(rest_payload_from_fixture 14-env-echo.yaml '"environment": {"TEST_VAR": "golden_value"}')"

  compare_rest_test "15-workdir" \
    -X POST "$BASE/api/tasks" \
    -H "Content-Type: application/json" \
    -d "$(rest_payload_from_fixture 15-workdir-pwd.yaml '"working_directory": "/tmp"')"

  compare_rest_test "21-bad-yaml" \
    -X POST "$BASE/api/tasks" \
    -H "Content-Type: application/json" \
    -d "$(rest_payload_from_fixture 21-no-command.yaml)"

  compare_rest_test "status-empty" \
    "$BASE/api/tasks/status"

  compare_rest_test "limiter-status-empty" \
    "$BASE/api/tasks/limiter-status"

  # Clean up daemon
  kill $DAEMON_PID 2>/dev/null || true
  wait $DAEMON_PID 2>/dev/null || true
  trap - EXIT
fi

# --- Summary ---
TOTAL=$((PASS_COUNT + FAIL_COUNT))
echo ""
echo "==============================="
echo "Results: ${PASS_COUNT}/${TOTAL} passed, ${FAIL_COUNT} failed"
echo "==============================="

if [ "$FAIL_COUNT" -gt 0 ]; then
  exit 1
fi
