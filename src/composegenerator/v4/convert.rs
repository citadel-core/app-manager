use super::{
    permissions, types,
    types::PortMapElement,
    utils::{get_host_port, get_main_container, validate_cmd},
};
use crate::{
    bmap,
    composegenerator::{
        compose::types::StringOrIntOrBool,
        output::types::{ComposeSpecification, NetworkEntry, Service},
        types::Permissions,
    },
};
use crate::{
    composegenerator::types::OutputMetadata,
    utils::{find_env_vars, flatten},
};
use std::collections::{BTreeMap, HashMap};

use crate::composegenerator::types::ResultYml;
use anyhow::{bail, Result};

fn get_main_port(
    containers: &HashMap<String, types::Container>,
    main_container: &str,
    port_map: &Option<HashMap<String, Vec<PortMapElement>>>,
) -> Result<u16> {
    let mut result: u16 = 0;
    for service_name in containers.keys() {
        let original_definition = containers.get(service_name).unwrap();
        if service_name != main_container && original_definition.port.is_some() {
            bail!("port: is not supported for containers other than the main container");
        }

        if let Some(internal_port) = original_definition.port {
            if service_name != main_container {
                bail!("port: is not supported for containers other than the main container");
            }
            let public_port: Option<&PortMapElement>;
            let fake_port = PortMapElement {
                internal_port,
                public_port: internal_port,
                dynamic: false,
            };
            if let Some(real_port_map) = port_map {
                if real_port_map.get(service_name).is_none() {
                    bail!(
                        "Container {} not found or invalid in port map",
                        service_name
                    );
                }
                let ports = real_port_map.get(service_name).unwrap();
                public_port = get_host_port(ports, internal_port);
            } else {
                public_port = Some(&fake_port);
            }
            if public_port.is_some() {
                result = internal_port;
                break;
            } else {
                bail!("Main container port not found in port map");
            }
        } else if service_name == main_container {
            let empty_vec = Vec::<PortMapElement>::with_capacity(0);
            if let Some(real_port) = port_map
                .clone()
                .unwrap_or_default()
                .get(service_name)
                .unwrap_or(&empty_vec)
                .iter()
                .find(|elem| elem.dynamic)
            {
                result = real_port.internal_port;
            } else if port_map.is_none() {
                result = 3000;
            } else {
                bail!("A port is required for the main container");
            }
        }
    }

    Ok(result)
}

fn configure_ports(
    containers: &HashMap<String, types::Container>,
    main_container: &str,
    output: &mut ComposeSpecification,
    port_map: &Option<HashMap<String, Vec<PortMapElement>>>,
) -> Result<()> {
    let services = output.services.as_mut().unwrap();
    for (service_name, service) in services {
        let original_definition = containers.get(service_name).unwrap();
        if service_name != main_container && original_definition.port.is_some() {
            bail!("port: is not supported for containers other than the main container",);
        }

        if let Some(internal_port) = original_definition.port {
            if service_name != main_container {
                bail!("port: is not supported for containers other than the main container",);
            }
            let public_port: Option<&PortMapElement>;
            let fake_port = PortMapElement {
                internal_port,
                public_port: internal_port,
                dynamic: false,
            };
            if let Some(real_port_map) = port_map {
                if real_port_map.get(service_name).is_none() {
                    bail!(
                        "Container {} not found or invalid in port map",
                        service_name
                    );
                }
                let ports = real_port_map.get(service_name).unwrap();
                public_port = get_host_port(ports, internal_port);
            } else {
                public_port = Some(&fake_port);
            }
            if let Some(port_map_elem) = public_port {
                service
                    .ports
                    .push(format!("{}:{}", port_map_elem.public_port, internal_port));
            } else {
                bail!("Main container port not found in port map");
            }
        } else if service_name == main_container {
            let empty_vec = Vec::<PortMapElement>::with_capacity(0);
            if port_map.is_some()
                && !port_map
                    .as_ref()
                    .unwrap()
                    .get(service_name)
                    .unwrap_or(&empty_vec)
                    .iter()
                    .any(|elem| elem.dynamic)
            {
                bail!("A port is required for the main container");
            }
        }
        if let Some(required_ports) = &original_definition.required_ports {
            if let Some(tcp_ports) = &required_ports.tcp {
                for port in tcp_ports {
                    service.ports.push(format!("{}:{}", port.0, port.1));
                }
            }
            if let Some(udp_ports) = &required_ports.udp {
                for port in udp_ports {
                    service.ports.push(format!("{}:{}/udp", port.0, port.1));
                }
            }
        }
    }

    Ok(())
}

fn define_ip_addresses(
    app_name: &str,
    containers: &HashMap<String, types::Container>,
    main_container: &str,
    output: &mut ComposeSpecification,
) -> Result<()> {
    let services = output.services.as_mut().unwrap();
    for (service_name, service) in services {
        if containers
            .get(service_name)
            .unwrap()
            .assign_fixed_ip
            .unwrap_or(true)
        {
            service.networks = Some(bmap! {
                "default" => NetworkEntry {
                    ipv4_address: Some(format!("$APP_{}_{}_IP", app_name.to_string().to_uppercase().replace('-', "_"), service_name.to_uppercase().replace('-', "_")))
                }
            })
        } else if service_name == main_container {
            bail!("Network can not be disabled for the main container");
        }
    }

    Ok(())
}

fn validate_service(
    app_name: &str,
    permissions: &mut Vec<String>,
    service: &types::Container,
    replace_env_vars: &HashMap<String, String>,
    result: &mut Service,
) -> Result<()> {
    if let Some(entrypoint) = &service.entrypoint {
        validate_cmd(app_name, entrypoint, permissions)?;
        result.entrypoint = Some(entrypoint.to_owned());
    }
    if let Some(command) = &service.command {
        validate_cmd(app_name, command, permissions)?;
        result.command = Some(command.to_owned());
    }
    if let Some(env) = &service.environment {
        result.environment = Some(BTreeMap::<String, StringOrIntOrBool>::new());
        let result_env = result.environment.as_mut().unwrap();
        for value in env {
            let val = match value.1 {
                StringOrIntOrBool::String(val) => {
                    let env_vars = find_env_vars(val);
                    for env_var in &env_vars {
                        if !permissions::is_allowed_by_permissions(app_name, env_var, permissions) {
                            bail!("Env var {} not allowed by permissions", env_var);
                        }
                    }
                    let mut val = val.to_owned();
                    if !env_vars.is_empty() {
                        let to_replace = replace_env_vars
                            .iter()
                            .filter(|(key, _)| env_vars.contains(&key.as_str()));
                        for (env_var, replacement) in to_replace {
                            let syntax_1 = "$".to_string() + env_var;
                            let syntax_2 = format!("${{{}}}", env_var);
                            val = val.replace(&syntax_1, replacement);
                            val = val.replace(&syntax_2, replacement);
                        }
                    }
                    StringOrIntOrBool::String(val)
                }
                StringOrIntOrBool::Int(int) => StringOrIntOrBool::Int(*int),
                StringOrIntOrBool::Bool(bool) => StringOrIntOrBool::Bool(*bool),
            };

            result_env.insert(value.0.to_owned(), val);
        }
    }
    if service.network_mode.is_some() {
        if !permissions.contains(&"network".to_string()) {
            // To preserve compatibility, this is only a warning, but we add the permission to the output
            tracing::warn!("App defines network-mode, but does not request the network permission");
            permissions.push("network".to_string());
        }
        result.network_mode = service.network_mode.to_owned();
    }
    if let Some(caps) = &service.cap_add {
        let mut cap_add = Vec::<String>::new();
        for cap in caps {
            match cap.to_lowercase().as_str() {
                "cap-net-raw" | "cap-net-admin" => {
                    if !permissions.contains(&"network".to_string()) {
                        bail!("App defines a network capability, but does not request the network permission");
                    }
                    cap_add.push(cap.to_owned());
                }
                _ => bail!("App defines unknown capability: {}", cap),
            }
        }
        result.cap_add = Some(cap_add);
    }
    Ok(())
}

fn convert_volumes(
    containers: &HashMap<String, types::Container>,
    permissions: &[String],
    output: &mut ComposeSpecification,
) -> Result<()> {
    let services = output.services.as_mut().unwrap();
    for (service_name, service) in services {
        let original_definition = containers.get(service_name).unwrap();
        if let Some(mounts) = &original_definition.mounts {
            if let Some(data_mounts) = &mounts.data {
                for (host_path, container_path) in data_mounts {
                    if host_path.contains("..") {
                        bail!("A data dir to mount is not allowed to contain '..'");
                    }
                    let mount_host_dir: String = if !host_path.starts_with('/') {
                        "/".to_owned() + host_path
                    } else {
                        host_path.clone()
                    };
                    service.volumes.push(format!(
                        "${{APP_DATA_DIR}}{}:{}",
                        mount_host_dir, container_path
                    ));
                }
            }

            if let Some(bitcoin_mount) = &mounts.bitcoin {
                if !permissions.contains(&"bitcoind".to_string()) {
                    bail!("bitcoin mount defined by container without Bitcoin permissions",);
                }
                service
                    .volumes
                    .push(format!("${{BITCOIN_DATA_DIR}}:{}", bitcoin_mount));
            }

            if let Some(lnd_mount) = &mounts.lnd {
                if !permissions.contains(&"lnd".to_string()) {
                    bail!("lnd mount defined by container without LND permissions");
                }
                service
                    .volumes
                    .push(format!("${{LND_DATA_DIR}}:{}", lnd_mount));
            }

            if let Some(c_lightning_mount) = &mounts.c_lightning {
                if !permissions.contains(&"c-lightning".to_string()) {
                    bail!(
                        "c-lightning mount defined by container without Core Lightning permissions",
                    );
                }
                service
                    .volumes
                    .push(format!("${{C_LIGHTNING_DATA_DIR}}:{}", c_lightning_mount));
            }
        }
    }

    Ok(())
}

fn get_hidden_services(
    app_name: &str,
    containers: HashMap<String, types::Container>,
    main_container: &str,
    main_port: u16,
    ip_addresses: &HashMap<String, String>,
) -> String {
    let mut result = String::new();
    for service_name in containers.keys() {
        let original_definition = containers.get(service_name).unwrap();
        if original_definition.network_mode == Some("host".to_string()) {
            continue;
        }
        let app_name_uppercase = app_name.to_uppercase().replace('-', "_");
        let service_name_uppercase = service_name.to_uppercase().replace('-', "_");
        let app_name_slug = app_name.to_lowercase().replace('_', "-");
        let service_name_slug = service_name.to_lowercase().replace('_', "-");
        if service_name == main_container {
            let hidden_service_string = format!(
                "HiddenServiceDir /var/lib/tor/app-{}\nHiddenServicePort 80 {}:{}\n",
                app_name_slug,
                ip_addresses
                    .get(&format!(
                        "APP_{}_{}_IP",
                        app_name_uppercase, service_name_uppercase
                    ))
                    .unwrap_or(&format!("<app-{}-{}-ip>", app_name_slug, service_name_slug)),
                main_port
            );
            result += hidden_service_string.as_str();
        }
        if let Some(hidden_services) = &original_definition.hidden_services {
            match hidden_services {
                types::HiddenServices::PortMap(simple_map) => {
                    if service_name != main_container {
                        let hidden_service_string = format!(
                            "HiddenServiceDir /var/lib/tor/app-{}-{}\n",
                            app_name_slug, service_name_slug
                        );
                        result += hidden_service_string.as_str();
                    }
                    for port in simple_map {
                        let port_string = format!(
                            "HiddenServicePort {} {}:{}\n",
                            port.0,
                            ip_addresses
                                .get(&format!(
                                    "APP_{}_{}_IP",
                                    app_name_uppercase, service_name_uppercase
                                ))
                                .unwrap_or(&format!(
                                    "<app-{}-{}-ip>",
                                    app_name_slug, service_name_slug
                                )),
                            port.1
                        );
                        result += port_string.as_str();
                    }
                }
                types::HiddenServices::LayeredMap(layered_map) => {
                    for element in layered_map {
                        let hidden_service_string = format!(
                            "HiddenServiceDir /var/lib/tor/app-{}-{}\n",
                            app_name_slug,
                            element.0.to_lowercase().replace('_', "-")
                        );
                        result += hidden_service_string.as_str();
                        for port in element.1 {
                            let port_string = format!(
                                "HiddenServicePort {} {}:{}\n",
                                port.0,
                                ip_addresses
                                    .get(&format!(
                                        "APP_{}_{}_IP",
                                        app_name_uppercase, service_name_uppercase
                                    ))
                                    .unwrap_or(&format!(
                                        "<app-{}-{}-ip>",
                                        app_name_slug, service_name_slug
                                    )),
                                port.1
                            );
                            result += port_string.as_str();
                        }
                    }
                }
            }
        }
    }

    result
}

fn get_i2p_tunnels(
    app_name: &str,
    containers: HashMap<String, types::Container>,
    main_container: &str,
    main_port: u16,
    ip_addresses: &HashMap<String, String>,
) -> String {
    let mut result = String::new();
    for service_name in containers.keys() {
        let original_definition = containers.get(service_name).unwrap();
        if original_definition.network_mode == Some("host".to_string()) {
            continue;
        }
        let app_name_uppercase = app_name.to_uppercase().replace('-', "_");
        let service_name_uppercase = service_name.to_uppercase().replace('-', "_");
        let app_name_slug = app_name.to_lowercase().replace('_', "-");
        let service_name_slug = service_name.to_lowercase().replace('_', "-");
        if service_name == main_container {
            let hidden_service_string = format!(
                "[app-{}-{}]\nhost = {}\nport = {}\nkeys = app-{}-{}.dat\n",
                app_name_slug,
                service_name_slug,
                ip_addresses
                    .get(&format!(
                        "APP_{}_{}_IP",
                        app_name_uppercase, service_name_uppercase
                    ))
                    .unwrap_or(&format!("<app-{}-{}-ip>", app_name_slug, service_name_slug)),
                main_port,
                app_name_slug,
                service_name_slug
            );
            result += hidden_service_string.as_str();
        }
        if original_definition.hidden_services.is_some() {
            tracing::info!("Multi-port hidden services are not yet supported for I2P on Citadel!");
        }
    }

    result
}

fn get_missing_dependencies(required: &[Permissions], installed: &[String]) -> Vec<Permissions> {
    let mut missing = Vec::<Permissions>::new();
    for requirement in required {
        match requirement {
            Permissions::OneDependency(dep) => {
                if !installed.contains(dep) {
                    missing.push(Permissions::OneDependency(dep.to_owned()));
                }
            }
            Permissions::AlternativeDependency(deps) => {
                if !deps.iter().any(|dep| installed.contains(dep)) {
                    missing.push(Permissions::AlternativeDependency(deps.to_owned()));
                }
            }
        }
    }
    missing
}

pub fn convert_config(
    app_name: &str,
    app: types::AppYml,
    port_map: &Option<HashMap<String, HashMap<String, Vec<PortMapElement>>>>,
    installed_services: &Option<Vec<String>>,
    ip_addresses: &Option<HashMap<String, String>>,
) -> Result<ResultYml> {
    let mut spec: ComposeSpecification = ComposeSpecification {
        services: Some(BTreeMap::new()),
    };
    let spec_services = spec.services.get_or_insert(BTreeMap::new());
    let mut permissions = flatten(app.metadata.permissions.clone());

    let main_service = get_main_container(&app)?;
    let mut app_port_map: Option<HashMap<String, Vec<PortMapElement>>> = None;
    if let Some(port_map) = port_map {
        if let Some(app_port_map_entry) = port_map.get(app_name) {
            let mut entry = app_port_map_entry.clone();
            if let Some(ref implements) = app.metadata.implements {
                if let Some(implement_port_map_entry) = port_map.get(implements) {
                    for (key, value) in implement_port_map_entry {
                        if entry.get(key).is_none() {
                            entry.insert(key.to_owned(), value.clone());
                        } else {
                            entry.get_mut(key).unwrap().extend(value.clone());
                        }
                    }
                }
            }
            app_port_map = Some(entry);
        }
    }
    let main_port = get_main_port(&app.services, &main_service, &app_port_map)?;

    // Required for dynamic ports
    let env_var = format!(
        "APP_{}_{}_PORT",
        app_name.replace('-', "_").to_uppercase(),
        main_service.to_uppercase()
    );

    let replace_env_vars = HashMap::<String, String>::from([
        (env_var, main_port.to_string()),
        ("ELECTRUM_IP".to_string(), "${APP_ELECTRUM_IP}".to_string()),
        ("ELECTRUM_PORT".to_string(), "50001".to_string()),
    ]);

    // Copy all properties that are the same in docker-compose.yml and need no or only a simple validation
    for (service_name, service) in &app.services {
        let base_result = Service {
            image: Some(service.image.clone()),
            restart: service.restart.clone(),
            stop_grace_period: service.stop_grace_period.clone(),
            stop_signal: service.stop_signal.clone(),
            user: service.user.clone(),
            init: service.init,
            depends_on: service.depends_on.clone(),
            extra_hosts: service.extra_hosts.clone(),
            working_dir: service.working_dir.clone(),
            ports: Vec::new(),
            volumes: Vec::new(),
            ..Default::default()
        };
        spec_services.insert(service_name.to_string(), base_result);
        validate_service(
            app_name,
            &mut permissions,
            service,
            &replace_env_vars,
            spec_services.get_mut(service_name).unwrap(),
        )?;
    }
    // We can now finalize the process by parsing some of the remaining values
    configure_ports(&app.services, &main_service, &mut spec, &app_port_map)?;

    define_ip_addresses(app_name, &app.services, &main_service, &mut spec)?;

    convert_volumes(&app.services, &permissions, &mut spec)?;

    let mut main_port_host: Option<u16> = None;
    if let Some(converted_map) = app_port_map {
        main_port_host = Some(
            get_host_port(converted_map.get(&main_service).unwrap(), main_port)
                .unwrap()
                .public_port,
        );
    }

    let missing_deps = get_missing_dependencies(
        &app.metadata.permissions,
        installed_services.as_ref().unwrap_or(&vec![]),
    );
    let mut metadata = OutputMetadata {
        id: app_name.to_string(),
        name: app.metadata.name,
        version: app.metadata.version,
        category: app.metadata.category,
        tagline: app.metadata.tagline,
        developers: app.metadata.developers,
        description: app.metadata.description,
        permissions: app.metadata.permissions,
        repo: app.metadata.repo,
        support: app.metadata.support,
        gallery: app.metadata.gallery,
        path: app.metadata.path,
        default_password: app.metadata.default_password,
        tor_only: app.metadata.tor_only,
        update_containers: app.metadata.update_containers,
        implements: app.metadata.implements,
        version_control: app.metadata.version_control,
        compatible: missing_deps.is_empty(),
        missing_dependencies: None,
        port: main_port_host.unwrap_or(main_port),
        internal_port: main_port,
        release_notes: app.metadata.release_notes,
    };
    if !missing_deps.is_empty() {
        metadata.missing_dependencies = Some(missing_deps);
    }

    let mut ips = HashMap::new();
    if let Some(ip_addresses) = ip_addresses {
        ips = ip_addresses.clone();
    }

    let result = ResultYml {
        spec,
        new_tor_entries: get_hidden_services(
            app_name,
            app.services.clone(),
            &main_service,
            main_port,
            &ips,
        ),
        new_i2p_entries: get_i2p_tunnels(app_name, app.services, &main_service, main_port, &ips),
        metadata,
    };

    // And we're done
    Ok(result)
}

#[cfg(test)]
mod test {
    use super::convert_config;
    use crate::{
        bmap,
        composegenerator::{
            output::types::{ComposeSpecification, NetworkEntry, Service},
            types::{OutputMetadata, Permissions, ResultYml},
            v4::types::{AppYml, Container, InputMetadata},
        },
        map,
    };

    use pretty_assertions::assert_eq;

    #[test]
    fn test_simple_app() {
        let example_app = AppYml {
            citadel_version: 4,
            metadata: InputMetadata {
                name: "Example app".to_string(),
                version: "1.0.0".to_string(),
                category: "Example category".to_string(),
                tagline: "The only example app for Citadel you will ever need".to_string(),
                developers: map! {
                    "Citadel team".to_string() => "runcitadel.space".to_string()
                },
                permissions: vec![Permissions::OneDependency("lnd".to_string())],
                repo: bmap! {
                    "Example repo".to_string() => "https://github.com/runcitadel/app-cli".to_string()
                },
                support: "https://t.me/citadeldevelopers".to_string(),
                description: "This is an example app that provides multiple features that you need on your node. These features include:\n\n- Example\n- Example\n- Example".to_string(),
                ..Default::default()
            },
            services: map! {
                "main" => Container {
                    image: "ghcr.io/runcitadel/example:main".to_string(),
                    user: Some("1000:1000".to_string()),
                    depends_on: Some(vec!["database".to_string()]),
                    port: Some(3000),
                    ..Default::default()
                },
                "database" => Container {
                    image: "ghcr.io/runcitadel/example-db:main".to_string(),
                    user: Some("1000:1000".to_string()),
                    ..Default::default()
                }
            }
        };
        let result = convert_config("example-app", example_app, &None, &None, &None);
        assert!(result.is_ok());
        let expected_result = ResultYml {
            spec: ComposeSpecification {
                services: Some(bmap! {
                    "main" => Service {
                        image: Some("ghcr.io/runcitadel/example:main".to_string()),
                        user: Some("1000:1000".to_string()),
                        depends_on: Some(vec!["database".to_string()]),
                        ports: vec!["3000:3000".to_string()],
                        networks: Some(bmap! {
                            "default" => NetworkEntry {
                                ipv4_address: Some("$APP_EXAMPLE_APP_MAIN_IP".to_string())
                            }
                        }),
                        ..Default::default()
                    },
                    "database" => Service {
                        image: Some("ghcr.io/runcitadel/example-db:main".to_string()),
                        user: Some("1000:1000".to_string()),
                        networks: Some(bmap! {
                            "default" => NetworkEntry {
                                ipv4_address: Some("$APP_EXAMPLE_APP_DATABASE_IP".to_string())
                            }
                        }),
                        ..Default::default()
                    }
                }),
                ..Default::default()

            },
            metadata: OutputMetadata {
                id: "example-app".to_string(),
                name: "Example app".to_string(),
                version: "1.0.0".to_string(),
                category: "Example category".to_string(),
                tagline: "The only example app for Citadel you will ever need".to_string(),
                developers: map! {
                    "Citadel team".to_string() => "runcitadel.space".to_string()
                },
                permissions: vec![Permissions::OneDependency("lnd".to_string())],
                repo: bmap! {
                    "Example repo".to_string() => "https://github.com/runcitadel/app-cli".to_string()
                },
                support: "https://t.me/citadeldevelopers".to_string(),
                description: "This is an example app that provides multiple features that you need on your node. These features include:\n\n- Example\n- Example\n- Example".to_string(),
                missing_dependencies: Some(vec![Permissions::OneDependency("lnd".to_string())]),
                compatible: false,
                port: 3000,
                internal_port: 3000,
                ..Default::default()
            },
            new_tor_entries: "HiddenServiceDir /var/lib/tor/app-example-app\nHiddenServicePort 80 <app-example-app-main-ip>:3000\n".to_string(),
            new_i2p_entries: "[app-example-app-main]\nhost = <app-example-app-main-ip>\nport = 3000\nkeys = app-example-app-main.dat\n".to_string(),
        };
        assert_eq!(expected_result, result.unwrap());
    }
}
