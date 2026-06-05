pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
#[cfg(test)]
mod tests {
    #[test]
    fn version_exists() {
        assert!(!super::version().is_empty());
    }
}
