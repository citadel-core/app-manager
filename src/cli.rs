use std::{
    collections::HashMap,
    io::{Read, Write},
    path::Path,
};

use serde::{Deserialize, Serialize};

use ::tera::Context;

use crate::{
    composegenerator::{
        convert_config, load_config_as_v4,
        types::OutputMetadata,
        v4::{
            types::{PortMapElement, PortPriority, StringOrMap},
            utils::{derive_entropy, get_main_container},
        },
    },
    constants::DEFAULT_CADDY_ENTRY_TEMPLATE,
};

use anyhow::Result;

mod preprocessing;
pub mod repos;
mod tera;
#[cfg(feature = "umbrel")]
#[allow(clippy::collapsible_match, clippy::unnecessary_unwrap)]
pub mod umbrel;

// A port map as used during creating the port map
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
struct PortCacheMapEntry {
    app: String,
    // Internal port
    internal_port: u16,
    container: String,
    dynamic: bool,
    implements: Option<String>,
    priority: PortPriority,
}

// Outside port -> app
type PortCacheMap = HashMap<u16, PortCacheMapEntry>;

static RESERVED_PORTS: [u16; 4] = [
    80,   // Dashboard
    433,  // Sometimes used by nginx with some setups
    443,  // Dashboard SSL
    8333, // Bitcoin Core P2P
];

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HttpsOptions {
    #[serde(default = "bool::default")]
    agreed_lets_encrypt_tos: bool,
    // user_email: String,
    app_domains: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserJson {
    #[serde(rename = "installedApps")]
    installed_apps: Vec<String>,
    https: Option<HttpsOptions>,
}

pub fn convert_dir(citadel_root: &str) -> Result<()> {
    let citadel_root = Path::new(&citadel_root);
    let apps = std::fs::read_dir(citadel_root.join("apps")).expect("Error reading apps directory!");
    let apps = apps.filter(|entry| {
        let entry = entry.as_ref().expect("Error reading app directory!");
        let path = entry.path();

        path.is_dir()
    });

    let mut services = Vec::<String>::new();
    let mut https_options = None;
    let user_json = std::fs::File::open(citadel_root.join("db").join("user.json"));
    if let Ok(user_json) = user_json {
        let user_json = serde_json::from_reader::<_, UserJson>(user_json);
        if let Ok(user_json) = user_json {
            services = user_json.installed_apps;
            https_options = user_json.https;
        }
    }
    services.append(&mut vec!["bitcoind".to_string()]);

    let mut citadel_seed = None;

    let citadel_seed_file = citadel_root.join("db").join("citadel-seed").join("seed");

    if citadel_seed_file.exists() {
        let mut citadel_seed_file = std::fs::File::open(citadel_seed_file).unwrap();
        let mut citadel_seed_str = String::new();
        citadel_seed_file
            .read_to_string(&mut citadel_seed_str)
            .unwrap();
        citadel_seed = Some(citadel_seed_str);
    }

    let ip_addresses_map_file = citadel_root.join("apps").join("ips.yml");
    let mut ip_map: HashMap<String, String> = HashMap::new();
    let mut current_suffix: u8 = 20;
    if ip_addresses_map_file.exists() {
        let ip_addresses_map_file = std::fs::File::open(ip_addresses_map_file.clone()).unwrap();
        let ip_addresses_map: HashMap<String, String> =
            serde_yaml::from_reader(ip_addresses_map_file).unwrap();
        ip_map = ip_addresses_map;
        current_suffix += ip_map.len() as u8;
    }
    // Later used for port assignment
    let mut port_map = HashMap::<String, HashMap<String, Vec<PortMapElement>>>::new();
    let mut port_map_cache: PortCacheMap = HashMap::new();
    let port_map_file = citadel_root.join("apps").join("ports.yml");
    let port_cache_map_file = citadel_root.join("apps").join("ports.cache.yml");
    if port_cache_map_file.exists() {
        let port_cache_map_file = std::fs::File::open(port_cache_map_file.clone()).unwrap();
        let port_cache_map_file: PortCacheMap =
            serde_yaml::from_reader(port_cache_map_file).expect("Failed to load port map!");
        port_map_cache = port_cache_map_file;
    }

    let mut validate_port = |app: &str,
                             container: &str,
                             suggested_port: u16,
                             priority: PortPriority,
                             dynamic: bool,
                             implements: Option<String>| {
        let get_new_port = |app: &str, container: &str, mut suggested_port: u16| -> u16 {
            while RESERVED_PORTS.contains(&suggested_port)
                || port_map_cache.contains_key(&suggested_port)
            {
                if let Some(cache_entry) = port_map_cache.get(&suggested_port) {
                    if cache_entry.app == app && cache_entry.container == container {
                        return suggested_port;
                    }
                }
                suggested_port += 1;
            }

            suggested_port
        };
        if let Some(key) = port_map_cache.get(&suggested_port) {
            if (key.app == app && key.container == container)
                || (key.implements == implements && container == "service")
            {
                return;
            }
            if key.priority > priority {
                // Move the existing app to a new port
                let new_port = get_new_port(&key.app, &key.container, suggested_port);
                let new_port_map = port_map_cache.remove(&suggested_port).unwrap();
                port_map_cache.insert(new_port, new_port_map);
                // And insert the new app
                port_map_cache.insert(
                    suggested_port,
                    PortCacheMapEntry {
                        app: app.to_string(),
                        internal_port: suggested_port,
                        container: container.to_string(),
                        dynamic,
                        implements,
                        priority,
                    },
                );
            } else {
                // Move the new app to a new port
                let new_port = get_new_port(app, container, suggested_port);
                port_map_cache.insert(
                    new_port,
                    PortCacheMapEntry {
                        app: app.to_string(),
                        internal_port: if dynamic { new_port } else { suggested_port },
                        container: container.to_string(),
                        dynamic,
                        implements,
                        priority,
                    },
                );
            }
        } else {
            port_map_cache.insert(
                suggested_port,
                PortCacheMapEntry {
                    app: app.to_string(),
                    internal_port: suggested_port,
                    container: container.to_string(),
                    dynamic,
                    implements,
                    priority,
                },
            );
        }
    };

    if citadel_seed.is_none() {
        tracing::warn!("Citadel does not seem to be set up yet!");
    }

    preprocessing::preprocess_apps(citadel_root, &citadel_root.join("apps"))
        .expect("Preprocessing apps failed");

    let mut data_dirs = HashMap::new();
    for app in apps {
        let app = app.expect("Error reading app directory!");
        let app_id = app.file_name();
        let app_id = app_id.to_str().unwrap();
        let app_yml = app.path().join("app.yml");
        let Ok(app_yml) = std::fs::File::open(app_yml) else {
            tracing::error!("Missing app.yml for app {}", app_id);
            continue;
        };
        let app_yml = load_config_as_v4(app_yml, &Some(&services.to_vec()));
        let Ok(app_yml) = app_yml else {
            tracing::error!("Error processing app.yml: {}", app_yml.unwrap_err());
            continue;
        };

        //Part 2: IP & Port assignment, also save data dirs
        let main_container = get_main_container(&app_yml.services)?;
        let has_service = app_yml.services.contains_key("service");
        for (service_name, service) in &app_yml.services {
            let ip_name = format!(
                "APP_{}_{}_IP",
                app_id.to_uppercase().replace('-', "_"),
                service_name.to_uppercase().replace('-', "_")
            );
            if let std::collections::hash_map::Entry::Vacant(e) = ip_map.entry(ip_name) {
                if current_suffix == 255 {
                    panic!("Too many apps!");
                }
                let ip = "10.21.21.".to_owned() + current_suffix.to_string().as_str();
                e.insert(ip);
                current_suffix += 1;
            }
            if let Some(main_port) = service.port {
                validate_port(
                    app_id,
                    service_name,
                    main_port,
                    service.port_priority.unwrap_or(PortPriority::Optional),
                    false,
                    app_yml.metadata.implements.clone(),
                );
            } else if main_container == service_name {
                validate_port(
                    app_id,
                    service_name,
                    3000,
                    PortPriority::Optional,
                    true,
                    app_yml.metadata.implements.clone(),
                );
            }
            if let Some(ports) = &service.required_ports {
                if let Some(tcp_ports) = &ports.tcp {
                    for host_port in tcp_ports.keys() {
                        validate_port(
                            app_id,
                            service_name,
                            *host_port,
                            PortPriority::Required,
                            false,
                            app_yml.metadata.implements.clone(),
                        );
                    }
                }
                if let Some(udp_ports) = &ports.udp {
                    for host_port in udp_ports.keys() {
                        validate_port(
                            app_id,
                            service_name,
                            *host_port,
                            PortPriority::Required,
                            false,
                            app_yml.metadata.implements.clone(),
                        );
                    }
                }
            }
            if let Some(mounts) = &service.mounts {
                if let Some(shared_data) = mounts.get("shared_data") {
                    if let StringOrMap::String(_) = shared_data {
                        tracing::warn!(
                            "App {} defines a string instead of an hashmap as shared data mount",
                            app_id
                        );
                        continue;
                    } else if let StringOrMap::Map(map) = shared_data {
                        if map.len() != 1 {
                            tracing::warn!(
                                "App {} has multiple shared data mounts, this is not supported!",
                                app_id
                            );
                            continue;
                        }
                        if (has_service && service_name == "service")
                            || (!has_service && service_name == main_container)
                        {
                            data_dirs.insert(
                                app_id.to_lowercase().clone(),
                                map.keys().next().unwrap().clone(),
                            );
                        } else {
                            tracing::warn!("App either has no service container and a shared_data mount in a container that is not the main container, or it has a service container and a shared_data mount in a container that is not the service container. This is not supported!");
                        }
                    }
                }
            }
        }
    }
    // Part 3: Convert port cache map to port map
    for (port_number, cache_entry) in port_map_cache.clone() {
        let mut key = cache_entry.app;
        if cache_entry.implements.is_some() && cache_entry.container == "service" {
            key = cache_entry.implements.unwrap();
        }
        if !port_map.contains_key(&key) {
            port_map.insert(key.clone(), HashMap::new());
        }
        let app_port_map = port_map.get_mut(&key).unwrap();
        if !app_port_map.contains_key(&cache_entry.container) {
            app_port_map.insert(cache_entry.container.clone(), Vec::new());
        }
        let service_port_map = app_port_map.get_mut(&cache_entry.container).unwrap();
        service_port_map.push(PortMapElement {
            dynamic: cache_entry.dynamic,
            internal_port: cache_entry.internal_port,
            public_port: port_number,
        });
    }
    // Part 4: Write port map to file
    {
        let mut port_map_file =
            std::fs::File::create(port_map_file).expect("Error opening port map file!");
        port_map_file
            .write_all(serde_yaml::to_string(&port_map).unwrap().as_bytes())
            .expect("Error writing port map file!");
        let mut port_cache_map_file =
            std::fs::File::create(port_cache_map_file).expect("Error opening port cache map file!");
        port_cache_map_file
            .write_all(serde_yaml::to_string(&port_map_cache).unwrap().as_bytes())
            .expect("Error writing port cache map file!");
        let ip_map_file =
            std::fs::File::create(ip_addresses_map_file).expect("Error opening ip map file!");
        serde_yaml::to_writer(ip_map_file, &ip_map).expect("Error writing ip map file!");
    }

    // Part 5: Save IP addresses
    {
        let mut env_string = String::new();
        // Load the existing env file
        if let Ok(mut env_file) = std::fs::File::open(citadel_root.join(".env")) {
            env_file
                .read_to_string(&mut env_string)
                .expect("Error reading env file!");
        }
        for (key, value) in &ip_map {
            let to_append = format!("{}={}", key, value);
            if !env_string.contains(&to_append) {
                env_string.push_str(&(to_append + "\n"));
            }
        }
        for (key, value) in &data_dirs {
            let to_append = format!(
                "{}_DATA_DIR={}",
                key.to_uppercase().replace('-', "_"),
                value
            );
            if !env_string.contains(&to_append) {
                env_string.push_str(&(to_append + "\n"));
            }
        }
        let mut env_file =
            std::fs::File::create(citadel_root.join(".env")).expect("Error opening env file!");
        env_file
            .write_all(env_string.as_bytes())
            .expect("Error writing env file!");
    }

    // Part 6: Loop through the appps again and run the actual conversion process
    let apps = std::fs::read_dir(citadel_root.join("apps")).expect("Error reading apps directory!");
    let mut app_registry: Vec<OutputMetadata> = Vec::new();
    let mut virtual_apps: HashMap<String, Vec<String>> = HashMap::new();

    let mut tor_entries: Vec<String> = Vec::new();
    let mut i2p_entries: Vec<String> = Vec::new();

    let mut caddy_entries = HashMap::new();

    for app in apps {
        let app = app.expect("Error reading app directory!");
        let app_id = app.file_name();
        let app_id = app_id.to_str().unwrap();
        let app_yml_path = app.path().join("app.yml");
        let docker_compose_yml_path = app.path().join("docker-compose.yml");
        // Skip if app.yml does not exist
        if !app_yml_path.exists() {
            // Delete docker-compose.yml if it exists
            if docker_compose_yml_path.exists() {
                std::fs::remove_file(docker_compose_yml_path)
                    .expect("Error deleting docker-compose.yml!");
            }
            continue;
        }
        let app_yml = std::fs::File::open(app_yml_path).expect("Error opening app.yml!");
        let conversion_result = convert_config(
            app_id,
            app_yml,
            &Some(port_map.clone()),
            &Some(services.clone()),
            &Some(ip_map.clone()),
        );
        if let Ok(result_data) = conversion_result {
            let mut docker_compose_yml_file = std::fs::File::create(docker_compose_yml_path)
                .expect("Error opening docker-compose.yml!");
            serde_yaml::to_writer(&mut docker_compose_yml_file, &result_data.spec)
                .expect("Error writing docker-compose.yml!");
            tor_entries.push(result_data.new_tor_entries + "\n");
            i2p_entries.push(result_data.new_i2p_entries + "\n");
            let mut metadata = result_data.metadata;
            if metadata.default_password.clone().unwrap_or_default() == "$APP_SEED" {
                if let Some(ref citadel_seed) = citadel_seed {
                    metadata.default_password = Some(derive_entropy(
                        citadel_seed,
                        format!("app-{}-seed", app_id).as_str(),
                    ));
                } else {
                    metadata.default_password = Some("Please reboot your node, default password does not seem to be available yet.".to_string());
                }
            }
            if let Some(ref implements) = metadata.implements {
                if let std::collections::hash_map::Entry::Vacant(entry) =
                    virtual_apps.entry(implements.clone())
                {
                    entry.insert(vec![app_id.to_string()]);
                } else {
                    virtual_apps
                        .get_mut(implements)
                        .unwrap()
                        .push(app_id.to_string());
                }
            }
            app_registry.push(metadata);
            caddy_entries.insert(app_id.to_owned(), result_data.caddy_entries);
        } else {
            // Delete docker-compose.yml if it exists
            if docker_compose_yml_path.exists() {
                std::fs::remove_file(docker_compose_yml_path)?;
            }
            tracing::error!(
                "Error converting app.yml for app {}: {}",
                app_id,
                conversion_result.unwrap_err()
            );
        }
    }

    // Part 7: Save registry & virtual apps
    {
        let app_registry_file = citadel_root.join("apps").join("registry.json");
        let mut app_registry_file = std::fs::File::create(app_registry_file)?;
        serde_json::to_writer(&mut app_registry_file, &app_registry)
            .expect("Error writing registry.json!");
        let virtual_apps_file = citadel_root.join("apps").join("virtual-apps.json");
        let mut virtual_apps_file = std::fs::File::create(virtual_apps_file)?;
        serde_json::to_writer(&mut virtual_apps_file, &virtual_apps)?;

        let tor_entries_file = citadel_root.join("tor").join("torrc-apps");
        let tor_entries_file_2 = citadel_root.join("tor").join("torrc-apps-2");
        let tor_entries_file_3 = citadel_root.join("tor").join("torrc-apps-3");
        let mut tor_entries_file = std::fs::File::create(tor_entries_file)?;
        let mut tor_entries_file_2 = std::fs::File::create(tor_entries_file_2)?;
        let mut tor_entries_file_3 = std::fs::File::create(tor_entries_file_3)?;
        // Split entries into 3 groups of the same size
        let mut current_file = 1;

        for entry in tor_entries {
            if current_file == 1 {
                tor_entries_file.write_all(entry.as_bytes())?;
                current_file = 2;
            } else if current_file == 2 {
                tor_entries_file_2.write_all(entry.as_bytes())?;
                current_file = 3;
            } else if current_file == 3 {
                tor_entries_file_3.write_all(entry.as_bytes())?;
                current_file = 1;
            }
        }
        let i2p_entries_dir = citadel_root.join("i2p").join("tunnels.d");
        std::fs::create_dir_all(i2p_entries_dir.clone())?;
        let i2p_entries_file = i2p_entries_dir.join("apps.conf");
        let mut i2p_entries_file = std::fs::File::create(i2p_entries_file)?;
        i2p_entries_file.write_all(i2p_entries.join("\n").as_bytes())?;
    }

    // Part 8: Preprocess config jinja files
    preprocessing::preprocess_config_files(citadel_root, &citadel_root.join("apps"))?;

    // Part 9: Configure caddy
    {
        let caddy_file = citadel_root.join("caddy").join("Caddyfile");
        let caddy_entry_template = citadel_root.join("templates").join("Caddyfile-entry.jinja");
        if !caddy_entry_template.is_file() {
            let mut caddy_entry_template_file = std::fs::File::create(&caddy_entry_template)
                .expect("Error opening Caddyfile-entry.jinja!");
            caddy_entry_template_file
                .write_all(DEFAULT_CADDY_ENTRY_TEMPLATE.as_bytes())
                .expect("Error writing Caddyfile-entry.jinja!");
        }
        let caddy_entry_tmpl = std::fs::read_to_string(caddy_entry_template)
            .expect("Error reading Caddyfile-entry.jinja!");
        let mut data_to_append = String::new();
        for (app_id, entries) in caddy_entries {
            for entry in entries {
                let mut tera_context = Context::new();
                tera_context.insert("PUBLIC_ADDR", &format!(":{}", entry.public_port));
                tera_context.insert("INTERNAL_PORT", &entry.internal_port);
                let ip_name = format!(
                    "APP_{}_{}_IP",
                    app_id.to_uppercase().replace('-', "_"),
                    entry.container_name.to_uppercase().replace('-', "_")
                );
                let ip = ip_map.get(&ip_name).unwrap();
                tera_context.insert("CONTAINER_IP", &ip);
                let processed_entry =
                    ::tera::Tera::one_off(&caddy_entry_tmpl, &tera_context, false)
                        .expect("Error rendering Caddyfile-entry.jinja!");
                data_to_append.push_str(&processed_entry);
                if entry.is_primary
                    && https_options.is_some()
                    && https_options.as_ref().unwrap().agreed_lets_encrypt_tos
                    && https_options
                        .as_ref()
                        .unwrap()
                        .app_domains
                        .contains_key(&app_id)
                {
                    let domain = https_options
                        .as_ref()
                        .unwrap()
                        .app_domains
                        .get(&app_id)
                        .unwrap();
                    let mut tera_context = Context::new();
                    tera_context.insert("PUBLIC_ADDR", domain);
                    tera_context.insert("INTERNAL_PORT", &entry.internal_port);
                    let ip_name = format!(
                        "APP_{}_{}_IP",
                        app_id.to_uppercase().replace('-', "_"),
                        entry.container_name.to_uppercase().replace('-', "_")
                    );
                    let ip = ip_map.get(&ip_name).unwrap();
                    tera_context.insert("CONTAINER_IP", &ip);
                    let processed_entry =
                        ::tera::Tera::one_off(&caddy_entry_tmpl, &tera_context, false)
                            .expect("Error rendering Caddyfile-entry.jinja!");
                    data_to_append.push_str(&processed_entry);
                }
            }
        }
        let mut caddy_file = std::fs::OpenOptions::new()
            .append(true)
            .open(caddy_file)
            .expect("Error opening Caddyfile!");
        caddy_file
            .write_all(data_to_append.as_bytes())
            .expect("Error writing Caddyfile!");
    }

    Ok(())
}
