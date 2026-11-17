//! The `spokenpath-core` binary, the command-line entry point to the Rust core.

fn version_line() -> String {
    format!("spokenpath-core {}", env!("CARGO_PKG_VERSION"))
}

fn main() {
    println!("{}", version_line());
}

#[cfg(test)]
mod tests {
    use super::version_line;

    #[test]
    fn version_line_names_the_binary_and_its_version() {
        assert_eq!(
            version_line(),
            concat!("spokenpath-core ", env!("CARGO_PKG_VERSION"))
        );
    }
}
