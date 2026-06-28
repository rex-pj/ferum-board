use ferum_infrastructure::plugins::circuit_breaker::CircuitBreaker;

#[test]
fn starts_closed() {
    let cb = CircuitBreaker::new(3, 60, 300);
    assert!(!cb.is_open());
}

#[test]
fn below_threshold_stays_closed() {
    let cb = CircuitBreaker::new(3, 60, 300);
    cb.record_failure();
    cb.record_failure();
    assert!(!cb.is_open());
}

#[test]
fn exactly_at_threshold_opens_circuit_and_returns_true() {
    let cb = CircuitBreaker::new(3, 60, 300);
    cb.record_failure();
    cb.record_failure();
    let opened = cb.record_failure();
    assert!(opened, "record_failure should signal true when the circuit first opens");
    assert!(cb.is_open());
}

#[test]
fn success_resets_counter_and_closes_circuit() {
    let cb = CircuitBreaker::new(3, 60, 300);
    cb.record_failure();
    cb.record_failure();
    cb.record_failure();
    assert!(cb.is_open());
    cb.record_success();
    assert!(!cb.is_open());
}

#[test]
fn after_success_reset_new_failure_streak_can_reopen() {
    let cb = CircuitBreaker::new(2, 60, 300);
    cb.record_failure();
    cb.record_failure();
    cb.record_success();
    assert!(!cb.is_open());
    cb.record_failure();
    let reopened = cb.record_failure();
    assert!(reopened);
    assert!(cb.is_open());
}

#[test]
fn only_first_failure_at_threshold_signals_open() {
    let cb = CircuitBreaker::new(2, 60, 300);
    cb.record_failure();
    let first = cb.record_failure();
    let second = cb.record_failure();
    assert!(first);
    assert!(!second, "subsequent failures while already open must not re-signal");
}

#[test]
fn zero_reset_duration_means_circuit_appears_closed_immediately() {
    let cb = CircuitBreaker::new(1, 60, 0);
    let opened = cb.record_failure();
    assert!(opened, "record_failure still signals the open event");
    assert!(!cb.is_open(), "is_open() returns false because reset elapsed immediately");
}
