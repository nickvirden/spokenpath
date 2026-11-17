//! The versioned JSON snapshot that a capture layer writes and the Rust core reads, which is the contract between capture and core.

#[cfg(test)]
mod tests {
    #[test]
    fn snapshot_tests_run_inline() {
        insta::assert_snapshot!(env!("CARGO_PKG_NAME"), @"spokenpath_snapshot");
    }
}
