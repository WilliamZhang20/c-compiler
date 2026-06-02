//! Profile-guided optimization (PGO) support.
//!
//! Uses a simple text profile format (no external libraries required):
//! ```text
//! function_name:block_index count
//! ```
//!
//! `-fprofile-generate` instruments basic-block entry counters.
//! `-fprofile-use` reads a profile file to guide block layout.

use ir::{Function, IRProgram, Terminator, BlockId};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub type BlockProfile = HashMap<(String, usize), u64>;

/// Load a profile file produced by `-fprofile-generate`.
pub fn load_profile(path: &Path) -> Result<BlockProfile, String> {
    let text = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read profile '{}': {}", path.display(), e))?;
    let mut profile = BlockProfile::new();
    for (line_no, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() != 2 {
            return Err(format!(
                "Invalid profile line {}: expected 'func:block count'",
                line_no + 1
            ));
        }
        let name_parts: Vec<&str> = parts[0].split(':').collect();
        if name_parts.len() != 2 {
            return Err(format!("Invalid profile key '{}'", parts[0]));
        }
        let func = name_parts[0].to_string();
        let block: usize = name_parts[1]
            .parse()
            .map_err(|_| format!("Invalid block index in '{}'", parts[0]))?;
        let count: u64 = parts[1]
            .parse()
            .map_err(|_| format!("Invalid count on line {}", line_no + 1))?;
        profile.insert((func, block), count);
    }
    Ok(profile)
}

/// Write profile counters to a file.
pub fn write_profile(path: &Path, profile: &BlockProfile) -> Result<(), String> {
    let mut lines: Vec<String> = profile
        .iter()
        .map(|((func, block), count)| format!("{}:{} {}", func, block, count))
        .collect();
    lines.sort();
    let body = lines.join("\n");
    fs::write(path, body)
        .map_err(|e| format!("Failed to write profile '{}': {}", path.display(), e))
}

/// Return block execution counts for layout (block id → count).
pub fn block_counts_for_function(func: &Function, profile: &BlockProfile) -> HashMap<BlockId, u64> {
    let mut counts = HashMap::new();
    for block in &func.blocks {
        let key = (func.name.clone(), block.id.0);
        if let Some(&c) = profile.get(&key) {
            counts.insert(block.id, c);
        }
    }
    counts
}

/// Reorder blocks using profile counts (hot blocks first after entry).
pub fn layout_with_profile(func: &mut Function, profile: &BlockProfile) {
    if func.blocks.len() <= 2 {
        return;
    }
    let counts = block_counts_for_function(func, profile);
    if counts.is_empty() {
        return;
    }

    let mut ordered = vec![func.entry_block];
    let mut visited = std::collections::HashSet::from([func.entry_block]);

    let mut remaining: Vec<BlockId> = func
        .blocks
        .iter()
        .map(|b| b.id)
        .filter(|id| !visited.contains(id))
        .collect();
    remaining.sort_by_key(|id| std::cmp::Reverse(counts.get(id).copied().unwrap_or(0)));

    for id in remaining {
        if visited.insert(id) {
            ordered.push(id);
        }
    }

    if ordered.len() == func.blocks.len() {
        let mut new_blocks = Vec::with_capacity(func.blocks.len());
        for id in ordered {
            if let Some(idx) = func.blocks.iter().position(|b| b.id == id) {
                new_blocks.push(func.blocks[idx].clone());
            }
        }
        func.blocks = new_blocks;
    }
}

/// Counter global symbol for a basic block when generating profile instrumentation.
pub fn profile_counter_name(func: &str, block: BlockId) -> String {
    format!("__profc_{}_{}", func, block.0)
}

/// Whether a function terminator ends control flow (no fall-through successors for layout).
pub fn is_exit_terminator(term: &Terminator) -> bool {
    matches!(
        term,
        Terminator::Ret(_) | Terminator::Unreachable | Terminator::IndirectBr { .. }
    )
}

/// Apply PGO layout pass to all functions when profile data is available.
pub fn apply_profile_layout(prog: &mut IRProgram, profile: &BlockProfile) {
    for func in &mut prog.functions {
        layout_with_profile(func, profile);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_profile_file() {
        let mut p = BlockProfile::new();
        p.insert(("main".to_string(), 0), 100);
        p.insert(("main".to_string(), 1), 5);
        let path = std::env::temp_dir().join("cc_test.prof");
        write_profile(&path, &p).unwrap();
        let loaded = load_profile(&path).unwrap();
        assert_eq!(loaded[ &("main".to_string(), 0)], 100);
        let _ = fs::remove_file(path);
    }
}
