use std::collections::{BTreeMap, HashMap};

use anyhow::{bail, Result};
use tracing::warn;

use crate::bmap;
use crate::composegenerator::compose::types::{
    Command, ComposeSpecification, EnvVars, StringOrIntOrBool,
};
use crate::composegenerator::types::Permissions;
use crate::composegenerator::umbrel::types::Metadata;
use crate::composegenerator::v4::types::{
    AppYml, Container, InputMetadata as CitadelMetadata, PortsDefinition, StringOrMap,
};
use crate::utils::find_env_vars;

pub fn convert_metadata(metadata: Metadata) -> CitadelMetadata {
    let deps: Vec<Permissions> = metadata
        .dependencies
        .into_iter()
        .map(|dep| -> Permissions {
            match dep.as_str() {
                "lightning" => Permissions::OneDependency("lnd".to_string()),
                "bitcoin" => Permissions::OneDependency("bitcoind".to_string()),
                "electrs" => Permissions::OneDependency("electrum".to_string()),
                _ => Permissions::OneDependency(dep),
            }
        })
        .collect();
    CitadelMetadata {
        name: metadata.name,
        version: metadata.version.clone(),
        repo: bmap! {
            "Public" => metadata.repo
        },
        support: metadata.support,
        category: metadata.category,
        tagline: metadata.tagline,
        permissions: deps,
        developers: bmap! {
            metadata.developer => metadata.website
        },
        gallery: metadata.gallery,
        path: metadata.path,
        default_username: metadata.default_username,
        default_password: if metadata.deterministic_password {
            Some("$APP_SEED".to_string())
        } else {
            metadata.default_password
        },
        tor_only: metadata.tor_only,
        update_containers: None,
        description: metadata.description,
        implements: None,
        version_control: None,
        release_notes: if let Some(release_notes) = metadata.release_notes {
            Some(BTreeMap::from([(metadata.version, release_notes)]))
        } else {
            None
        },
    }
}

fn replace_env_vars(mut string: String, env_vars: &HashMap<String, String>) -> String {
    if string.contains("APP_BITCOIN_NETWORK") {
        string = string.replace("APP_BITCOIN_NETWORK", "BITCOIN_NETWORK");
    }
    if string.contains("APP_BITCOIN_RPC_PORT") {
        string = string.replace("APP_BITCOIN_RPC_PORT", "BITCOIN_RPC_PORT");
    }
    if string.contains("APP_BITCOIN_P2P_PORT") {
        string = string.replace("APP_BITCOIN_P2P_PORT", "BITCOIN_P2P_PORT");
    }
    if string.contains("APP_BITCOIN_RPC_USER") {
        string = string.replace("APP_BITCOIN_RPC_USER", "BITCOIN_RPC_USER");
    }
    if string.contains("APP_BITCOIN_RPC_PASS") {
        string = string.replace("APP_BITCOIN_RPC_PASS", "BITCOIN_RPC_PASS");
    }
    if string.contains("APP_BITCOIN_NODE_IP") {
        string = string.replace("APP_BITCOIN_NODE_IP", "BITCOIN_IP");
    }
    if string.contains("APP_LIGHTNING_NODE_GRPC_PORT") {
        string = string.replace("APP_LIGHTNING_NODE_GRPC_PORT", "LND_GRPC_PORT");
    }
    if string.contains("APP_LIGHTNING_NODE_REST_PORT") {
        string = string.replace("APP_LIGHTNING_NODE_REST_PORT", "LND_REST_PORT");
    }
    if string.contains("APP_LIGHTNING_NODE_IP") {
        string = string.replace("APP_LIGHTNING_NODE_IP", "LND_IP");
    }
    if string.contains("APP_ELECTRS_NODE_IP") {
        string = string.replace("APP_ELECTRS_NODE_IP", "APP_ELECTRUM_IP");
    }
    if string.contains("APP_ELECTRS_NODE_PORT") {
        string = string.replace("APP_ELECTRS_NODE_PORT", "APP_ELECTRUM_PORT");
    }
    let str_clone = string.clone();
    let env_vars_in_str = find_env_vars(&str_clone);
    for env_var in env_vars_in_str {
        if let Some(env_var_value) = env_vars.get(env_var) {
            string = string
                .replace(&format!("${env_var}"), env_var_value)
                .replace(&format!("${{{env_var}}}"), env_var_value);
        }
    }
    string
}

pub fn convert_compose(
    compose: ComposeSpecification,
    metadata: Metadata,
    env_vars: &HashMap<String, String>,
) -> Result<AppYml> {
    let services = compose.services.unwrap();
    let mut result_services: HashMap<String, Container> = HashMap::new();
    let has_main = services.contains_key("main");
    let mut deps = Vec::<String>::new();
    for service in services {
        let mut service_name = service.0;
        let service_def = service.1;
        // We don't have an app_proxy
        if service_name == "app_proxy" || service_name == "tor" {
            continue;
        }
        if service_name == "web" && !has_main {
            service_name = "main".to_string();
        }
        let mut mounts = BTreeMap::new();
        let mut new_data_mounts = BTreeMap::<String, String>::new();
        for volume in service_def.volumes {
            // Convert mounts using env vars to real mounts
            // For example, if a volume is "${APP_DATA_DIR}/thing:/data",
            // we add set "/thing" of the mounts.data hashmap to "/data"
            let split = volume.split(':').collect::<Vec<&str>>();
            if split.len() != 2 && split.len() != 3 {
                continue;
            }
            let volume_name = split[0];
            let volume_path = split[1];
            if volume_name.contains("${APP_DATA_DIR}") || volume_name.contains("$APP_DATA_DIR") {
                let volume_name_without_prefix = volume_name
                    .replace("${APP_DATA_DIR}", "")
                    .replace("$APP_DATA_DIR", "");
                let volume_name_without_prefix = volume_name_without_prefix.trim_start_matches('/');
                new_data_mounts.insert(
                    volume_name_without_prefix.to_string(),
                    volume_path.to_string(),
                );
            } else if volume_name.contains("APP_LIGHTNING_NODE_DATA_DIR") {
                mounts.insert(
                    "lnd".to_string(),
                    StringOrMap::String(volume_path.to_string()),
                );
            } else if volume_name.contains("APP_BITCOIN_DATA_DIR") {
                mounts.insert(
                    "bitcoin".to_string(),
                    StringOrMap::String(volume_path.to_string()),
                );
            } else if volume_name.contains("APP_CORE_LIGHTNING_REST_CERT_DIR") {
                bail!("C Lightning mounts are not supported yet");
            } else {
                bail!("Unsupported mount found.");
            }
        }
        if !new_data_mounts.is_empty() {
            mounts.insert("data".to_string(), StringOrMap::Map(new_data_mounts));
        }
        let mut env: Option<HashMap<String, StringOrIntOrBool>> = Some(HashMap::new());
        let original_env = match service_def.environment {
            Some(env) => match env {
                EnvVars::List(list) => {
                    let mut map = HashMap::<String, StringOrIntOrBool>::new();
                    for val in list {
                        let mut split = val.split('=');
                        let Some(key) = split.next() else {
                            tracing::error!("Encountered invalid env var: {}", val);
                            continue;
                        };
                        let Some(value) = split.next() else {
                            tracing::error!("Encountered invalid env var: {}", val);
                            continue;
                        };
                        map.insert(
                            key.to_string(),
                            StringOrIntOrBool::String(value.to_string()),
                        );
                    }
                    map
                }
                EnvVars::Map(map) => map,
            },
            None => HashMap::<String, StringOrIntOrBool>::new(),
        };
        for (key, value) in original_env {
            let new_value = match value {
                StringOrIntOrBool::String(str) => {
                    let mut new_value = replace_env_vars(str.clone(), env_vars);
                    // If the APP_PASSWORD is also used, there could be a conflict otherwise
                    // For apps which don't use APP_PASSWORD, this can be reverted
                    if new_value.contains("APP_SEED") && metadata.deterministic_password {
                        new_value = new_value.replace("APP_SEED", "APP_SEED_2");
                    }
                    if new_value.contains("APP_PASSWORD") {
                        new_value = new_value.replace("APP_PASSWORD", "APP_SEED");
                    }
                    StringOrIntOrBool::String(new_value)
                }
                _ => value,
            };
            env.as_mut().unwrap().insert(key, new_value);
        }
        let mut new_cmd: Option<Command> = None;
        if let Some(command) = service_def.command {
            match command {
                Command::SimpleCommand(mut command) => {
                    command = replace_env_vars(command, env_vars);
                    if command.contains("APP_PASSWORD") {
                        // If the APP_SEED is also used, use APP_SEED_2 instead so the seed and the password are different
                        if command.contains("APP_SEED") {
                            command = command.replace("APP_SEED", "APP_SEED_2");
                        }
                        command = command.replace("APP_PASSWORD", "APP_SEED");
                    }
                    new_cmd = Some(Command::SimpleCommand(command));
                }
                Command::ArrayCommand(values) => {
                    let mut result = Vec::<String>::new();
                    for mut argument in values {
                        argument = replace_env_vars(argument, env_vars);
                        // If the APP_PASSWORD is also used, there could be a conflict otherwise
                        // For apps which don't use APP_PASSWORD, this can be reverted
                        if argument.contains("APP_SEED") {
                            argument = argument.replace("APP_SEED", "APP_SEED_2");
                        }
                        if argument.contains("APP_PASSWORD") {
                            argument = argument.replace("APP_PASSWORD", "APP_SEED");
                        }
                        result.push(argument);
                    }
                    new_cmd = Some(Command::ArrayCommand(result));
                }
            };
        }
        if let Some(caps) = &service_def.cap_add {
            if caps.contains(&"CAP_NET_ADMIN".to_string())
                || caps.contains(&"CAP_NET_RAW".to_string())
            {
                deps.push("network".to_string());
            }
        }
        if service_def.network_mode.is_some() {
            deps.push("network".to_string());
        }
        let mut required_tcp_ports: HashMap<u16, u16> = HashMap::new();
        let mut required_udp_ports: HashMap<u16, u16> = HashMap::new();
        let mut main_port = metadata.port;
        if service_def.ports.len() != 1 {
            for port in service_def.ports {
                let split = port.split(':').collect::<Vec<&str>>();
                if split.len() != 2 {
                    continue;
                }
                let mut host_port = split[0];
                let container_port = split[1].split('/').collect::<Vec<&str>>();

                // The first part is definitely the container port, the secon part is either a protocol or empty
                // Empty means tcp
                let mut real_container_port = container_port[0];
                let protocol = if container_port.len() == 1 {
                    "tcp"
                } else {
                    container_port[1]
                };
                let host_env_vars = find_env_vars(host_port);
                let container_env_vars = find_env_vars(real_container_port);
                if container_env_vars.len() == 2 {
                    warn!("Found two env vars in container port, this is not supported");
                    continue;
                }
                if container_env_vars.len() == 1
                    && container_env_vars[0]
                        == format!("APP_{}_PORT", metadata.id.to_uppercase().replace('-', "_"))
                {
                    let real_main_port = env_vars.get(container_env_vars[0]).unwrap();
                    main_port = real_main_port.parse::<u16>().unwrap();
                    continue;
                } else if container_env_vars.len() == 1 {
                    #[allow(unused_assignments)]
                    if env_vars.contains_key(container_env_vars[0]) {
                        let real_port = env_vars.get(container_env_vars[0]).unwrap();
                        real_container_port = real_port;
                        continue;
                    }
                }
                if host_env_vars.len() == 1
                    && host_env_vars[0]
                        != format!("APP_{}_PORT", metadata.id.to_uppercase().replace('-', "_"))
                {
                    #[allow(unused_assignments)]
                    if env_vars.contains_key(host_env_vars[0]) {
                        let real_port = env_vars.get(host_env_vars[0]).unwrap();
                        host_port = real_port;
                        continue;
                    }
                }
                if protocol == "tcp" {
                    required_tcp_ports.insert(
                        host_port.parse::<u16>().unwrap(),
                        real_container_port.parse::<u16>().unwrap(),
                    );
                } else if protocol == "udp" {
                    required_udp_ports.insert(
                        host_port.parse::<u16>().unwrap(),
                        real_container_port.parse::<u16>().unwrap(),
                    );
                } else {
                    unreachable!();
                }
            }
        }
        let new_service = Container {
            image: service_def.image.unwrap(),
            user: service_def.user,
            stop_grace_period: service_def.stop_grace_period,
            stop_signal: service_def.stop_signal,
            depends_on: service_def.depends_on,
            network_mode: service_def.network_mode,
            restart: service_def.restart,
            init: service_def.init,
            extra_hosts: service_def.extra_hosts,
            entrypoint: service_def.entrypoint,
            working_dir: None,
            command: new_cmd,
            environment: env,
            port: if service_name == "main" || service_name == "web" {
                Some(main_port)
            } else {
                None
            },
            port_priority: None,
            required_ports: if required_udp_ports.is_empty() && required_tcp_ports.is_empty() {
                None
            } else {
                Some(PortsDefinition {
                    tcp: if required_tcp_ports.is_empty() { None } else { Some(required_tcp_ports) },
                    udp: if required_udp_ports.is_empty() { None } else { Some(required_udp_ports) },
                    http: None,
                })
            },
            mounts: Some(mounts),
            assign_fixed_ip: if service_def.networks.is_some() {
                None
            } else {
                Some(false)
            },
            hidden_services: None,
            cap_add: service_def.cap_add,
            direct_tcp: false,
        };
        result_services.insert(service_name, new_service);
    }
    Ok(AppYml {
        citadel_version: 4,
        metadata: convert_metadata(metadata),
        services: result_services,
    })
}
