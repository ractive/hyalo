//! Focused runner for the existing cross-iteration safety fixtures.

use anyhow::{Context, Result, bail};
use std::process::Command;

use crate::workspace::workspace_root;

const SUITES: &[(&str, &str)] = &[
    (
        "prepared write and output preflight",
        "iteration290_foundations::",
    ),
    (
        "reader/writer framing budgets",
        "iteration291_document_consistency::",
    ),
    ("snapshot and graph refresh", "iteration292_coherence::"),
    (
        "mutation faults and whole-batch preflight",
        "iteration293_mutation_safety::",
    ),
    (
        "preview/apply hint equivalence",
        "iteration294_continuation_contracts::",
    ),
    (
        "graph/index parity and complete bulk writes",
        "iteration277_graph_parity_and_write_perf::",
    ),
];

pub fn run() -> Result<bool> {
    let root = workspace_root()?;
    let mut failures = Vec::new();
    let negative_control = std::env::var_os("HYALO_XTASK_BEHAVIORAL_ZERO_TEST_CONTROL").is_some();
    let mut executed = 0usize;
    for (index, (contract, configured_filter)) in SUITES.iter().enumerate() {
        let filter = if negative_control && index == 0 {
            "iteration295_deliberately_missing_suite::"
        } else {
            *configured_filter
        };
        let output = Command::new("cargo")
            .args([
                "test",
                "-q",
                "-p",
                "hyalo-cli",
                "--test",
                "e2e",
                filter,
                "--",
                "--test-threads=1",
            ])
            .current_dir(&root)
            .output()
            .with_context(|| format!("running {contract} suite"))?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        if output.status.success() {
            match executed_test_count(&stdout) {
                Ok(count) => {
                    executed += count;
                    println!(
                        "check-behavioral-contracts: PASS {contract} ({filter}, {count} tests)"
                    );
                }
                Err(error) => failures.push(format!(
                    "{contract} ({filter}) did not prove test execution: {error}\n{stdout}"
                )),
            }
        } else {
            failures.push(format!(
                "{contract} ({filter}) exited {}:\n{}{}",
                output.status,
                stdout,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    if failures.is_empty() {
        println!(
            "check-behavioral-contracts: PASS {} focused suites, {executed} tests executed",
            SUITES.len(),
        );
        Ok(true)
    } else {
        eprintln!(
            "check-behavioral-contracts: {} suite(s) failed:\n\n{}",
            failures.len(),
            failures.join("\n\n")
        );
        Ok(false)
    }
}

fn executed_test_count(stdout: &str) -> Result<usize> {
    let count = stdout
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("running ")?
                .strip_suffix(" tests")?
                .parse::<usize>()
                .ok()
        })
        .sum();
    if count == 0 {
        bail!("Cargo reported zero matching tests");
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::executed_test_count;

    #[test]
    fn rejects_zero_test_output() {
        assert!(executed_test_count("running 0 tests\n\ntest result: ok").is_err());
    }

    #[test]
    fn accepts_positive_test_output() {
        assert_eq!(executed_test_count("running 17 tests\n").unwrap(), 17);
    }
}
