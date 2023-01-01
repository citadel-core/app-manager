pub const BITCOIN_ENV_VARS: [&str; 10] = [
    "BITCOIN_IP",
    "BITCOIN_P2P_PORT",
    "BITCOIN_RPC_PORT",
    "BITCOIN_RPC_USER",
    "BITCOIN_RPC_PASS",
    "BITCOIN_RPC_AUTH",
    "BITCOIN_ZMQ_RAWBLOCK_PORT",
    "BITCOIN_ZMQ_RAWTX_PORT",
    "BITCOIN_ZMQ_HASHBLOCK_PORT",
    "BITCOIN_ZMQ_SEQUENCE_PORT",
];

pub const LND_ENV_VARS: [&str; 3] = ["LND_IP", "LND_GRPC_PORT", "LND_REST_PORT"];

pub const ELECTRUM_ENV_VARS: [&str; 2] = ["ELECTRUM_IP", "ELECTRUM_PORT"];

pub const C_LIGHTNING_ENV_VARS: [&str; 1] = ["C_LIGHTNING_IP"];

pub const ALWAYS_ALLOWED_ENV_VARS: [&str; 12] = [
    "TOR_PROXY_IP",
    "TOR_PROXY_PORT",
    "APP_DOMAIN",
    "APP_HIDDEN_SERVICE",
    "BITCOIN_NETWORK",
    "APP_SEED",
    "APP_SEED_1",
    "APP_SEED_2",
    "APP_SEED_3",
    "APP_SEED_4",
    "APP_SEED_5",
    "MANAGER_IP",
];

pub fn is_allowed_by_permissions(
    app_id: &str,
    env_var: &str,
    permissions: &[&String],
) -> bool {
    if ALWAYS_ALLOWED_ENV_VARS.contains(&env_var) {
        return true;
    } else if env_var.starts_with("BITCOIN") {
        return permissions.contains(&&"bitcoind".to_string())
            && BITCOIN_ENV_VARS.contains(&env_var);
    } else if env_var.starts_with("LND") {
        if env_var == "LND_IP" {
            tracing::warn!("The environment variable LND_IP is deprecated. Please use APP_LND_SERVICE_IP instead");
        }
        return permissions.contains(&&"lnd".to_string()) && LND_ENV_VARS.contains(&env_var);
    } else if env_var.starts_with("ELECTRUM") {
        tracing::warn!("Environment variables starting with ELECTRUM_ are deprecated. Please use APP_ELECTRUM_* instead");
        return permissions.contains(&&"electrum".to_string())
            && ELECTRUM_ENV_VARS.contains(&env_var);
    } else if env_var.starts_with("C_LIGHTNING") {
        tracing::warn!("Environment variables starting with C_LIGHTNING_ are deprecated. Please use APP_CORE_LN_* instead");
        return (permissions.contains(&&"c-lightning".to_string())
            || permissions.contains(&&"core-ln".to_string()))
            && C_LIGHTNING_ENV_VARS.contains(&env_var);
    } else if env_var.starts_with("APP_HIDDEN_SERVICE_") || env_var.starts_with("APP_SEED") {
        return true;
    } else if env_var == "DEVICE_HOSTNAME" || env_var == "DEVICE_HOSTS" {
        return permissions.contains(&&"network".to_string());
    } else if env_var.starts_with("APP_") {
        let mut split_result: Vec<&str> = env_var.split('_').collect();
        // Remove the APP_
        split_result.remove(0);
        // Remove the _IP / _PORT / _SHAREDSECRET
        split_result.pop();
        while app_id != split_result.join("-").to_lowercase()
            && !permissions.contains(&&split_result.join("-").to_lowercase())
        {
            // Remove stuff until we hit the end of the value
            split_result.pop();
            if split_result.is_empty() {
                return false;
            }
        }
        return true;
    }
    false
}

#[cfg(test)]
mod test {
    use super::is_allowed_by_permissions;

    #[test]
    fn allow_access_to_own_vars() {
        let result = is_allowed_by_permissions("example-app", "APP_EXAMPLE_APP_CONTAINER_IP", &[]);
        assert!(result);
        let result2 = is_allowed_by_permissions("example-app", "APP_SEED_5", &[]);
        assert!(result2);
    }

    #[test]
    fn dont_crash_with_weird_vars() {
        let result = is_allowed_by_permissions("example-app", "APP_EXAMPLEAPP", &[]);
        assert!(!result);
    }

    #[test]
    fn prevent_access_to_other_vars() {
        let result = is_allowed_by_permissions("example-app", "APP_ANOTHER_APP_CONTAINER_IP", &[]);
        assert!(!result);
    }

    #[test]
    fn allow_access_to_apps_with_permission() {
        let result = is_allowed_by_permissions(
            "example-app",
            "APP_ANOTHER_APP_CONTAINER_IP",
            &[&"another-app".to_string()],
        );
        assert!(result);
    }

    #[test]
    fn allow_access_to_builtins_with_permission() {
        let result =
            is_allowed_by_permissions("example-app", "BITCOIN_IP", &[&"bitcoind".to_string()]);
        assert!(result);
    }

    #[test]
    fn always_allow_certain_values() {
        let result = is_allowed_by_permissions("example-app", "BITCOIN_NETWORK", &[]);
        assert!(result);
    }

    #[test]
    fn allow_access_to_electrum_both_ways() {
        assert!(is_allowed_by_permissions(
            "example-app",
            "ELECTRUM_IP",
            &[&"electrum".to_string()]
        ));
        assert!(is_allowed_by_permissions(
            "example-app",
            "APP_ELECTRUM_IP",
            &[&"electrum".to_string()]
        ));
    }

    #[test]
    fn allow_cln_and_lnd_data_dirs() {
        assert!(is_allowed_by_permissions(
            "example-app",
            "APP_CORE_LN_DATA_DIR",
            &[&"core-ln".to_string()]
        ));
        assert!(is_allowed_by_permissions(
            "example-app",
            "APP_LND_DATA_DIR",
            &[&"lnd".to_string()]
        ));
    }
}
