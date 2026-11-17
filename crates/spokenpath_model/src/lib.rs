//! The engine-neutral model that a per-engine adapter builds from a snapshot.

#[cfg(test)]
mod tests {
    #[test]
    fn builds_as_spokenpath_model() {
        assert_eq!(env!("CARGO_PKG_NAME"), "spokenpath_model");
    }
}
