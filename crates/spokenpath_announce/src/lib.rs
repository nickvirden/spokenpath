//! Computes reading order and navigation indexes from the engine-neutral model and renders the transcript.

#[cfg(test)]
mod tests {
    #[test]
    fn builds_as_spokenpath_announce() {
        assert_eq!(env!("CARGO_PKG_NAME"), "spokenpath_announce");
    }
}
