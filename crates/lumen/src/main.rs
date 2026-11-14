//! Lumen command-line entry point. Rendering is not implemented yet.

/// The crate version, as recorded in `Cargo.toml`.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Formats the line the binary prints.
fn banner() -> String {
    format!("lumen {VERSION}")
}

fn main() {
    println!("{}", banner());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_names_the_crate_and_its_version() {
        assert_eq!(banner(), format!("lumen {}", env!("CARGO_PKG_VERSION")));
        assert!(banner().starts_with("lumen 0."));
    }
}
