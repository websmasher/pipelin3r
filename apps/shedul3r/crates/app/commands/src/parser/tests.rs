use super::*;

#[test]
fn parse_minimal_task() {
    let yaml = "name: test\ncommand: echo hello\n";
    let def = parse_task_definition(yaml);
    assert!(def.is_ok(), "should parse minimal task");
    let def = def.ok();
    assert!(def.is_some(), "should have a value");
    let def = def.as_ref();
    assert_eq!(def.map(|d| d.command.as_str()), Some("echo hello"));
}

#[test]
fn parse_run_alias() {
    let yaml = "name: test\nrun: echo world\n";
    let def = parse_task_definition(yaml);
    assert!(def.is_ok(), "should parse 'run' alias");
}

#[test]
fn missing_command_errors() {
    let yaml = "name: test\n";
    let result = parse_task_definition(yaml);
    assert!(result.is_err(), "should error on missing command");
}

#[test]
fn parse_human_durations() {
    let seconds = parse_duration("30s");
    assert!(seconds.is_ok(), "should parse 30s");
    assert_eq!(seconds.ok(), Some(Duration::from_secs(30)));

    let minutes = parse_duration("5m");
    assert!(minutes.is_ok(), "should parse 5m");
    assert_eq!(minutes.ok(), Some(Duration::from_secs(300)));

    let hours = parse_duration("1h");
    assert!(hours.is_ok(), "should parse 1h");
    assert_eq!(hours.ok(), Some(Duration::from_secs(3600)));
}

#[test]
fn parse_iso8601_durations() {
    let whole = parse_duration("PT30S");
    assert!(whole.is_ok(), "should parse PT30S");
    assert_eq!(whole.ok(), Some(Duration::from_secs(30)));

    let fractional = parse_duration("PT1.5S");
    assert!(fractional.is_ok(), "should parse PT1.5S");
    assert_eq!(fractional.ok(), Some(Duration::from_secs_f64(1.5)));
}

#[test]
fn parse_full_task() {
    let yaml = r"
name: my-task
command: echo hello
timeout: 30s
provider-id: shared-key
rate-limit:
  max-rate: 10
  window: 1s
retry:
  max-retries: 3
  initial-delay: 1s
  backoff-multiplier: 2.0
  max-delay: 10s
circuit-breaker:
  failure-rate-threshold: 50
  sliding-window-size: 10
  wait-duration-in-open-state: 30s
max-concurrent: 3
max-wait: 5m
";
    let def = parse_task_definition(yaml);
    assert!(def.is_ok(), "should parse full task: {def:?}");
    let td = def.ok();
    assert!(td.is_some(), "should have a value");
    let td_ref = td.as_ref();
    assert!(td_ref.is_some(), "ref should exist");
    assert_eq!(
        td_ref.and_then(|t| t.limiter_key.as_deref()),
        Some("shared-key")
    );
    assert!(
        td_ref.and_then(|t| t.rate_limit_config.as_ref()).is_some(),
        "should have rate limit config"
    );
    assert!(
        td_ref.and_then(|t| t.retry_config.as_ref()).is_some(),
        "should have retry config"
    );
    assert!(
        td_ref.and_then(|t| t.circuit_breaker_config.as_ref()).is_some(),
        "should have circuit breaker config"
    );
    assert!(
        td_ref.and_then(|t| t.bulkhead_config.as_ref()).is_some(),
        "should have bulkhead config"
    );
}

#[test]
fn parse_circuit_breaker_config() {
    let yaml = r"
name: cb-task
command: echo hello
circuit-breaker:
  failure-rate-threshold: 75
  sliding-window-size: 20
  wait-duration-in-open-state: 1m
";
    let def = parse_task_definition(yaml);
    assert!(def.is_ok(), "should parse circuit breaker config: {def:?}");
    let td = def.ok();
    let cb = td.as_ref().and_then(|t| t.circuit_breaker_config.as_ref());
    assert!(cb.is_some(), "should have circuit breaker config");
    let cb = cb.as_ref();
    assert_eq!(cb.map(|c| c.failure_rate_threshold), Some(75.0));
    assert_eq!(cb.map(|c| c.sliding_window_size), Some(20));
    assert_eq!(
        cb.map(|c| c.wait_duration_in_open_state),
        Some(Duration::from_secs(60))
    );
}

#[test]
fn parse_circuit_breaker_missing_threshold_errors() {
    let yaml = r"
name: cb-task
command: echo hello
circuit-breaker:
  sliding-window-size: 10
  wait-duration-in-open-state: 30s
";
    let result = parse_task_definition(yaml);
    assert!(
        result.is_err(),
        "missing failure-rate-threshold should error"
    );
}

#[test]
fn parse_circuit_breaker_missing_window_errors() {
    let yaml = r"
name: cb-task
command: echo hello
circuit-breaker:
  failure-rate-threshold: 50
  wait-duration-in-open-state: 30s
";
    let result = parse_task_definition(yaml);
    assert!(
        result.is_err(),
        "missing sliding-window-size should error"
    );
}

#[test]
fn parse_circuit_breaker_missing_wait_duration_errors() {
    let yaml = r"
name: cb-task
command: echo hello
circuit-breaker:
  failure-rate-threshold: 50
  sliding-window-size: 10
";
    let result = parse_task_definition(yaml);
    assert!(
        result.is_err(),
        "missing wait-duration-in-open-state should error"
    );
}

#[test]
fn task_without_circuit_breaker_has_none() {
    let yaml = "name: test\ncommand: echo hello\n";
    let def = parse_task_definition(yaml);
    assert!(def.is_ok(), "should parse without circuit breaker");
    let td = def.ok();
    assert!(
        td.as_ref().and_then(|t| t.circuit_breaker_config.as_ref()).is_none(),
        "should not have circuit breaker config when absent"
    );
}
