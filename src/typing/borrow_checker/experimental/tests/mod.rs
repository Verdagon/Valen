// Borrow-checker tests, re-enabled against the rewritten checker; their diagnostics now point at the
// use site. `util` holds the shared harness helpers.
mod attack_tests;
mod ellipsis_tests;
mod held_register_tests;
mod joint_argument_move_tests;
mod joint_argument_tests;
mod noalias_facts_tests;
#[cfg(not(feature = "rust_interop"))]
mod noalias_tests;
mod producer_gate_tests;
mod group_facts_tests;
mod robustness_tests;
mod same_group_aliasing_tests;
mod synthesized_callee_tests;
mod use_after_churn_tests;
mod walk_completeness_tests;
use crate::typing::test::borrow_checker::util;
