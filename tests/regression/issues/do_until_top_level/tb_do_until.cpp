// Placeholder TB for the issue #410 repro.
//
// Pre-fix: the design lowered to an infinite-loop FSM and this TB was
// used to observe `body_count_r` incrementing forever while `done_r`
// also asserted from cycle 0.
//
// Post-fix: `DoUntilRepro.arch` no longer compiles — the elaborator
// rejects `do … until` bodies that contain nested control flow (here,
// the inner `lock lk`). The Rust integration tests
//   `test_do_until_rejects_nested_lock`
//   `test_do_until_rejects_nested_wait`
//   `test_do_until_rejects_nested_for`
// (in `tests/integration_test.rs`) are the authoritative regression
// coverage for the rejection diagnostic. This file is kept so the
// directory remains self-contained.

int main() { return 0; }
