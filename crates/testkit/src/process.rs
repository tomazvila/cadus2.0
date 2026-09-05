//! The DSN helper of a process test: the same database as another cluster
//! role.

/// The DSN of `superuser_dsn` as `role`, with the empty password of trust
/// authentication.
///
/// The credentials before the `@` go; the scheme, the host, the port, and the
/// database path stay.
pub fn with_role(superuser_dsn: &str, role: &str) -> String {
    let (scheme, rest) = superuser_dsn
        .split_once("://")
        .expect("the DSN names a scheme");
    let host = rest.rsplit_once('@').map_or(rest, |(_, host)| host);
    format!("{scheme}://{role}@{host}")
}

#[cfg(test)]
mod tests {
    use super::with_role;

    #[test]
    fn the_role_replaces_the_credentials() {
        assert_eq!(
            with_role("postgresql://test:test@127.0.0.1:55435/db_1", "cadus_app"),
            "postgresql://cadus_app@127.0.0.1:55435/db_1"
        );
    }

    #[test]
    fn a_dsn_without_credentials_gains_the_role() {
        assert_eq!(
            with_role("postgresql://127.0.0.1/db_1", "cadus_app"),
            "postgresql://cadus_app@127.0.0.1/db_1"
        );
    }

    #[test]
    #[should_panic(expected = "the DSN names a scheme")]
    fn a_dsn_without_a_scheme_stops_the_test() {
        with_role("127.0.0.1/db_1", "cadus_app");
    }
}
