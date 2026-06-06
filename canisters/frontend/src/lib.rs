pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub const EXPECTED_HISTORIAN_DASHBOARD_METHOD: &str = "get_dashboard_state";
pub const EXPECTED_HISTORIAN_STATUS_METHOD: &str = "get_public_status";

#[cfg(test)]
mod tests {
    #[test]
    fn version_exists() {
        assert!(!super::version().is_empty());
    }

    #[test]
    fn frontend_points_at_historian_read_model() {
        assert_eq!(
            super::EXPECTED_HISTORIAN_DASHBOARD_METHOD,
            "get_dashboard_state"
        );
        assert_eq!(super::EXPECTED_HISTORIAN_STATUS_METHOD, "get_public_status");
    }
}
