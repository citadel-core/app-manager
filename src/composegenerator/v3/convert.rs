use super::types::Schema as AppYmlV3;
use crate::composegenerator::types::ResultYml;
use crate::composegenerator::v4::types::{PortMapElement, StringOrMap};
use crate::composegenerator::v4::{
    convert::convert_config as convert_config_v4, types as types_v4,
};
use crate::utils::flatten;
use anyhow::Result;
use std::collections::{BTreeMap, HashMap};

pub fn v3_to_v4(app: AppYmlV3, installed_services: &Option<&Vec<String>>) -> types_v4::AppYml {
    let repo = match app.metadata.repo {
        super::types::RepoDefinition::RepoUrl(url) => BTreeMap::from([("Public".to_string(), url)]),
        super::types::RepoDefinition::MultiRepo(map) => map,
    };
    let metadata = types_v4::InputMetadata {
        name: app.metadata.name,
        version: app.metadata.version,
        category: app.metadata.category,
        tagline: app.metadata.tagline,
        developers: app.metadata.developers,
        permissions: app.metadata.dependencies.clone().unwrap_or_default(),
        repo,
        support: app.metadata.support,
        gallery: app.metadata.gallery,
        path: app.metadata.path,
        default_username: None,
        default_password: app.metadata.default_password,
        tor_only: app.metadata.tor_only.unwrap_or(false),
        update_containers: None,
        description: app.metadata.description,
        implements: None,
        version_control: None,
        release_notes: None,
    };
    let mut services = HashMap::<String, types_v4::Container>::with_capacity(app.containers.len());
    let deps = app.metadata.dependencies.unwrap_or_default();
    let deps = flatten(&deps);
    'container_loop: for container in app.containers {
        if let Some(installed_services) = installed_services {
            for dependency in container.requires.clone().unwrap_or_default() {
                if !installed_services.contains(&dependency) {
                    continue 'container_loop;
                }
            }
        }
        let mut port_priority = None;
        if container.preferred_outside_port == container.port {
            port_priority = Some(types_v4::PortPriority::Recommended);
        }
        if container.requires_port.unwrap_or(false) {
            port_priority = Some(types_v4::PortPriority::Required);
        }
        let mut required_ports = None;
        if container.required_ports.is_some() || container.required_udp_ports.is_some() {
            let mut required_ports_def = types_v4::PortsDefinition {
                udp: None,
                tcp: None,
            };
            if let Some(tcp_ports) = container.required_ports {
                let mut map = HashMap::<u16, u16>::with_capacity(tcp_ports.capacity());
                for value in tcp_ports.iter() {
                    map.insert(*value, *value);
                }
                required_ports_def.tcp = Some(map);
            }
            if let Some(udp_ports) = container.required_udp_ports {
                let mut map = HashMap::<u16, u16>::with_capacity(udp_ports.capacity());
                for value in udp_ports.iter() {
                    map.insert(*value, *value);
                }
                required_ports_def.udp = Some(map);
            }
            required_ports = Some(required_ports_def);
        }
        let mut assign_fixed_ip = None;
        if container.no_network.unwrap_or(false) {
            assign_fixed_ip = Some(false);
        }
        let mut mounts = HashMap::new();
        let requires = container.requires.unwrap_or_default();
        let old_mounts = container.mounts.unwrap_or_default();
        if deps.contains(&&"lnd".to_string()) && !requires.contains(&"c-lightning".to_string()) {
            mounts.insert("lnd".to_string(), StringOrMap::String(old_mounts.lnd.unwrap_or_else(|| "/lnd".into())));
        }
        if deps.contains(&&"c-lightning".to_string()) && !requires.contains(&"lnd".to_string()) {
            mounts.insert("core-ln".to_string(),
            StringOrMap::String(old_mounts
                    .c_lightning
                    .unwrap_or_else(|| "/c-lightning".into()))
            );
        }
        if deps.contains(&&"bitcoin".to_string()) {
            mounts.insert("bitcoin".to_string(), StringOrMap::String(old_mounts.bitcoin.unwrap_or_else(|| "/bitcoin".into())));
        }
        let data_mounts = container.data.unwrap_or_default();
        let mut new_data_mounts = HashMap::<String, String>::with_capacity(
            data_mounts.capacity(),
        );
        for value in &data_mounts {
            let mut split = value.split(':');
            let Some(key) = split.next() else {
                tracing::error!("Encountered invalid mount: {}", value);
                continue;
            };
            let Some(value) = split.next() else {
                tracing::error!("Encountered invalid env var: {}", value);
                continue;
            };
            new_data_mounts
                .insert(key.to_string(), value.to_string());
        }
        if !new_data_mounts.is_empty() {
            mounts.insert("data".to_string(), StringOrMap::Map(new_data_mounts));
        }

        services.insert(
            container.name,
            types_v4::Container {
                image: container.image,
                user: container.user,
                stop_grace_period: container.stop_grace_period,
                stop_signal: container.stop_signal,
                depends_on: container.depends_on,
                network_mode: container.network_mode,
                restart: container.restart,
                init: container.init,
                extra_hosts: None,
                entrypoint: container.entrypoint,
                command: container.command,
                working_dir: None,
                environment: container.environment,
                port: container.port,
                port_priority,
                required_ports,
                mounts: Some(mounts),
                assign_fixed_ip,
                hidden_services: container.hidden_service_ports.map(|value| match value {
                    super::types::HiddenServices::PortMap(map) => {
                        types_v4::HiddenServices::PortMap(map)
                    }
                    super::types::HiddenServices::LayeredMap(map) => {
                        types_v4::HiddenServices::LayeredMap(map)
                    }
                    super::types::HiddenServices::LegacyLayeredMap(map) => {
                        let new_values = map.iter().map(|val| {
                            let hashmap = HashMap::from_iter(val.1.iter().map(|val| (*val, *val)));
                            (val.0.to_owned(), hashmap)
                        });
                        types_v4::HiddenServices::LayeredMap(HashMap::from_iter(new_values))
                    }
                    super::types::HiddenServices::LegacySinglePort(port) => {
                        types_v4::HiddenServices::PortMap(HashMap::from([(port, port)]))
                    }
                    super::types::HiddenServices::LegacyPortArray(ports) => {
                        let hashmap = HashMap::from_iter(ports.iter().map(|val| (*val, *val)));
                        types_v4::HiddenServices::PortMap(hashmap)
                    }
                    super::types::HiddenServices::LegacyMap(map) => {
                        let new_values = map
                            .iter()
                            .map(|(name, port)| (name.to_owned(), HashMap::from([(*port, *port)])));
                        types_v4::HiddenServices::LayeredMap(HashMap::from_iter(new_values))
                    }
                }),
                cap_add: None,
            },
        );
    }
    types_v4::AppYml {
        citadel_version: 4,
        metadata,
        services,
    }
}

pub fn convert_config(
    app_name: &str,
    app: AppYmlV3,
    port_map: &Option<HashMap<String, HashMap<String, Vec<PortMapElement>>>>,
    installed_services: &Vec<String>,
    ip_addresses: &Option<HashMap<String, String>>,
) -> Result<ResultYml> {
    convert_config_v4(
        app_name,
        v3_to_v4(app, &Some(installed_services)),
        port_map,
        &Some(installed_services.clone()),
        ip_addresses,
    )
}
