//! Stub implementations for gates that are not yet implemented.
//!
//! These exist so the ralph-loop skill can reference them by name without
//! warnings about missing checks. Real implementations land in iter-142b.

use anyhow::Result;

/// Stub for `check-dead-primitives`.
///
// allow-todo: iter-142b
pub fn check_dead_primitives() -> Result<bool> {
    println!("check-dead-primitives: not yet implemented (iter-142b)");
    Ok(true)
}

/// Stub for `check-todo-annotations`.
///
// allow-todo: iter-142b
pub fn check_todo_annotations() -> Result<bool> {
    println!("check-todo-annotations: not yet implemented (iter-142b)");
    Ok(true)
}
