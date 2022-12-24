pub const MINIMUM_COMPATIBLE_APP_MANAGER: &str = "";
pub const MINIMUM_COMPATIBLE_APP_YML: u8 = 3;
pub const NO_SEED_FOUND_FALLBACK_MSG: &str = "This app used APP_SEED, so it can't be converted at this stage. You should never see this message in any file. Also, don't put it in your Jiinja files to avoid breaking the app system.";
pub const DEFAULT_CADDY_ENTRY_TEMPLATE: &str = ":{{PUBLIC_PORT}} {
    reverse_proxy {{CONTAINER_IP}}:{{INTERNAL_PORT}}
}";
