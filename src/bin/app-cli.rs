use citadel_apps::composegenerator::types::OutputMetadata;
#[cfg(all(feature = "umbrel", feature = "dev-tools"))]
use citadel_apps::composegenerator::umbrel::types::Metadata as UmbrelMetadata;
use citadel_apps::composegenerator::v4::types::{AppYml, PortMapElement, PortPriority};
use citadel_apps::composegenerator::{convert_config, load_config, load_config_as_v4};
use citadel_apps::{
    composegenerator::v4::{permissions::is_allowed_by_permissions, utils::derive_entropy},
    utils::flatten,
};
#[cfg(feature = "dev-tools")]
use citadel_apps::{
    composegenerator::{
        compose::types::ComposeSpecification,
        types::ResultYml,
        v3::{convert::v3_to_v4, types::SchemaItemContainers},
    },
    updates::update_app,
};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::process::exit;
use tera::{Context, Tera};

#[derive(Subcommand, Debug)]
enum SubCommand {
    /// Convert a citadel app.yml to a result.yml file
    Convert {
        /// The citadel root dir
        citadel_root: String,
    },
    /// Get a JSON schema for the app.yml format
    #[cfg(feature = "dev-tools")]
    Schema {
        /// The version of the app.yml format to get the schema for
        /// (defaults to 4)
        #[clap(short, long, default_value = "4")]
        version: String,
    },
    /// Convert an Umbrel app (by app directory path) to a Citadel app.yml file
    /// Manual fixes may be required to make the app.yml work
    #[cfg(feature = "umbrel")]
    UmbrelToCitadel {
        /// The app directory to run this on
        app: String,
        /// The output file to save the result to
        output: String,
    },
    /// Validate a Citadel app.yml file and check if it could be parsed & converted
    #[cfg(feature = "dev-tools")]
    Validate {
        /// The app file to run this on
        app: String,
        /// The app's ID
        #[clap(short, long)]
        app_name: String,
    },
    /// Update the app inside an app.yml to its latest version
    #[cfg(feature = "dev-tools")]
    Update {
        /// The app file or directory to run this on
        app: String,
        /// A GitHub token
        #[clap(short, long)]
        token: Option<String>,
        /// Whether to include pre-releases
        #[clap(short, long)]
        include_prerelease: bool,
    },
    /// Convert an app.yml v3 to an app.yml v4
    /// v3 added implicit mounts of the bitcoin, lnd and CLN data directories, you can remove them from the output if they are not needed
    #[cfg(feature = "dev-tools")]
    V3ToV4 {
        /// The app file to run this on
        app: String,
    },
}

/// Manage apps on Citadel
#[derive(Parser)]
struct Cli {
    /// The subcommand to run
    #[clap(subcommand)]
    command: SubCommand,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserJson {
    #[serde(rename = "installedApps")]
    installed_apps: Vec<String>,
    // We ignore other properties for now because we do not need them
}

static RESERVED_PORTS: [u16; 6] = [
    // Dashboard
    80,    // Sometimes used by nginx with some setups
    433,   // Dashboard SSL
    443,   // Bitcoin Core P2P
    8333,  // LND gRPC
    10009, // LND REST
    8080,
];

#[cfg(feature = "dev-tools")]
async fn update_app_yml(path: &Path, include_prerelease: bool) {
    let app_yml = std::fs::File::open(path).expect("Error opening app definition!");
    let mut parsed_app_yml = load_config(app_yml).expect("Failed to parse app.yml");
    let update_result = update_app(&mut parsed_app_yml, include_prerelease).await;
    if update_result.is_err() {
        return;
    }
    match parsed_app_yml {
        citadel_apps::composegenerator::AppYmlFile::V4(app_yml) => {
            let writer = std::fs::File::create(path).expect("Error opening app definition!");
            serde_yaml::to_writer(writer, &app_yml).expect("Error saving app definition!");
        }
        citadel_apps::composegenerator::AppYmlFile::V3(app_yml) => {
            let writer = std::fs::File::create(path).expect("Error opening app definition!");
            serde_yaml::to_writer(writer, &app_yml).expect("Error saving app definition!");
        }
    }
}
#[tokio::main]
async fn main() {
    env_logger::init();
    let args: Cli = Cli::parse();
    match args.command {
        SubCommand::Convert { citadel_root } => {
            let citadel_root = Path::new(&citadel_root);
            let apps = std::fs::read_dir(citadel_root.join("apps"))
                .expect("Error reading apps directory!");
            let apps = apps.filter(|entry| {
                let entry = entry.as_ref().expect("Error reading app directory!");
                let path = entry.path();

                path.is_dir()
            });
            let mut services = Vec::<String>::new();
            let user_json = std::fs::File::open(citadel_root.join("db").join("user.json"));
            if let Ok(user_json) = user_json {
                let user_json = serde_json::from_reader::<_, UserJson>(user_json);
                if let Ok(user_json) = user_json {
                    services = user_json.installed_apps;
                }
            }
            services.append(&mut vec!["bitcoind".to_string(), "lnd".to_string()]);

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

            let mut env_vars = Vec::new();

            if let Ok(dot_env) = dotenv::from_filename_iter(citadel_root.join(".env")) {
                env_vars = dot_env.collect();
            }

            let ip_addresses_map_file = citadel_root.join("apps").join("ips.yml");
            let mut ip_map: HashMap<String, String> = HashMap::new();
            let mut current_suffix: u8 = 20;
            if ip_addresses_map_file.exists() {
                let ip_addresses_map_file = std::fs::File::open(ip_addresses_map_file).unwrap();
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

            let mut validate_port =
                |app: &str,
                 container: &str,
                 suggested_port: u16,
                 priority: &PortPriority,
                 dynamic: bool,
                 implements: Option<String>| {
                    let get_new_port =
                        |app: &str, container: &str, mut suggested_port: u16| -> u16 {
                            while RESERVED_PORTS.contains(&suggested_port)
                                || port_map_cache.contains_key(&suggested_port)
                            {
                                if let Some(cache_entry) = port_map_cache.get(&suggested_port) {
                                    if cache_entry.app == app && cache_entry.container == container
                                    {
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
                        if key.priority > *priority {
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
                                    priority: *priority,
                                },
                            );
                        } else {
                            // Move the new app to a new port
                            let new_port = get_new_port(app, container, suggested_port);
                            port_map_cache.insert(
                                new_port,
                                PortCacheMapEntry {
                                    app: app.to_string(),
                                    internal_port: suggested_port,
                                    container: container.to_string(),
                                    dynamic,
                                    implements,
                                    priority: *priority,
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
                                priority: *priority,
                            },
                        );
                    }
                };

            for app in apps {
                let app = app.expect("Error reading app directory!");
                let app_id = app.file_name();
                let app_id = app_id.to_str().unwrap();

                // Part 1: Process Tera templates
                {
                    let app_yml_jinja = app.path().join("app.yml.jinja");
                    if app_yml_jinja.exists() {
                        let mut context = Context::new();
                        context.insert("services", &services);
                        context.insert("app_name", app_id);
                        let mut tmpl = String::new();
                        std::fs::File::open(app_yml_jinja.clone())
                            .expect("Error opening app definition!")
                            .read_to_string(&mut tmpl)
                            .expect("Error reading app definition!");
                        let tmpl_result = Tera::one_off(tmpl.as_str(), &context, false)
                            .expect("Error running templating engine on app definition!");
                        let mut writer = std::fs::File::create(app_yml_jinja.with_extension(""))
                            .expect("Error opening app definition!");
                        writer
                            .write_all(tmpl_result.as_bytes())
                            .expect("Error saving file!");
                    }
                }
                let app_yml = std::fs::File::open(app.path().join("app.yml"))
                    .expect("Error opening app definition!");
                let app_yml = load_config_as_v4(app_yml, &Some(&services))
                    .expect("Error parsing app definition!");

                if !env_vars.is_empty() && citadel_seed.is_some() {
                    let citadel_seed = citadel_seed.as_ref().unwrap();
                    let other_jinja_files = std::fs::read_dir(app.path())
                        .expect("Error reading app directory!")
                        .filter(|entry| {
                            let entry = entry.as_ref().expect("Error reading app directory!");
                            let path = entry.path();
                            return path.is_file()
                                && path.extension().unwrap_or_default() == "jinja"
                                && path.file_name().unwrap_or_default() != "app.yml.jinja";
                        });
                    for jinja_file in other_jinja_files {
                        let jinja_file = jinja_file.expect("Error reading app directory!");
                        let output_file = jinja_file.path().with_extension("");
                        let mut context = Context::new();
                        context.insert("services", &services);
                        context.insert("app_name", app_id);

                        let permissions = flatten(app_yml.metadata.permissions.clone());
                        for item in &env_vars {
                            let (key, val) = item.as_ref().expect("Env var invalid");
                            if is_allowed_by_permissions(app_id, key, &permissions) {
                                context.insert(key, &val);
                            }
                        }
                        context.insert(
                            "APP_SEED",
                            &derive_entropy(citadel_seed, format!("app-{}-seed", app_id).as_str()),
                        );
                        for i in 1..6 {
                            context.insert(
                                format!("APP_SEED_{}", i),
                                &derive_entropy(
                                    citadel_seed,
                                    format!("app-{}-seed{}", app_id, i).as_str(),
                                ),
                            );
                        }
                        context.insert("APP_VERSION", &app_yml.metadata.version);

                        let mut tmpl = String::new();
                        std::fs::File::open(jinja_file.path())
                            .expect("Error opening app definition!")
                            .read_to_string(&mut tmpl)
                            .expect("Error reading app definition!");
                        let tmpl_result = Tera::one_off(tmpl.as_str(), &context, false)
                            .expect("Error running templating engine on config file!");
                        let mut writer =
                            std::fs::File::create(output_file).expect("Error opening config file!");
                        writer
                            .write_all(tmpl_result.as_bytes())
                            .expect("Error saving file!");
                    }
                } else {
                    eprintln!("Warning: Citadel does not seem to be set up")
                }

                //Part 2: IP & Port assignment
                {
                    for (service_name, service) in app_yml.services {
                        let ip_name = format!("APP_{}_{}_IP", app_id, service_name);
                        if let std::collections::hash_map::Entry::Vacant(e) = ip_map.entry(ip_name)
                        {
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
                                &service_name,
                                main_port,
                                &service.port_priority.unwrap_or(PortPriority::Optional),
                                false,
                                app_yml.metadata.implements.clone(),
                            );
                        }
                        if let Some(ports) = service.required_ports {
                            if let Some(tcp_ports) = ports.tcp {
                                for (host_port, _) in tcp_ports {
                                    validate_port(
                                        app_id,
                                        &service_name,
                                        host_port,
                                        &PortPriority::Required,
                                        false,
                                        app_yml.metadata.implements.clone(),
                                    );
                                }
                            }
                            if let Some(udp_ports) = ports.udp {
                                for (host_port, _) in udp_ports {
                                    validate_port(
                                        app_id,
                                        &service_name,
                                        host_port,
                                        &PortPriority::Required,
                                        false,
                                        app_yml.metadata.implements.clone(),
                                    );
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
                let mut port_cache_map_file = std::fs::File::create(port_cache_map_file)
                    .expect("Error opening port cache map file!");
                port_cache_map_file
                    .write_all(serde_yaml::to_string(&port_map_cache).unwrap().as_bytes())
                    .expect("Error writing port cache map file!");
            }

            // Part 5: Save IP addresses
            {
                let mut env_string = String::new();
                // Load the existing env file
                if let Ok(mut env_file) = std::fs::File::open(citadel_root.join("env")) {
                    env_file
                        .read_to_string(&mut env_string)
                        .expect("Error reading env file!");
                }
                for (key, value) in ip_map {
                    let to_append = format!("{}={}", key, value);
                    if !env_string.contains(&to_append) {
                        env_string.push_str(&(to_append + "\n"));
                    }
                }
                let mut env_file = std::fs::File::create(citadel_root.join("env"))
                    .expect("Error opening env file!");
                env_file
                    .write_all(env_string.as_bytes())
                    .expect("Error writing env file!");
            }

            // Part 6: Loop through the appps again and run the actual conversion process
            let apps = std::fs::read_dir(citadel_root.join("apps"))
                .expect("Error reading apps directory!");
            let mut app_registry: Vec<OutputMetadata> = Vec::new();

            let mut tor_entries: Vec<String> = Vec::new();
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
                );
                if let Ok(result_data) = conversion_result {
                    let mut docker_compose_yml_file =
                        std::fs::File::create(docker_compose_yml_path)
                            .expect("Error opening docker-compose.yml!");
                    serde_yaml::to_writer(&mut docker_compose_yml_file, &result_data.spec)
                        .expect("Error writing docker-compose.yml!");
                    tor_entries.push(result_data.new_tor_entries + "\n");
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
                    app_registry.push(metadata);
                } else {
                    // Delete docker-compose.yml if it exists
                    if docker_compose_yml_path.exists() {
                        std::fs::remove_file(docker_compose_yml_path)
                            .expect("Error deleting docker-compose.yml!");
                    }
                    eprintln!(
                        "Error converting app.yml for app {}: {}",
                        app_id,
                        conversion_result.err().unwrap()
                    );
                }
            }

            // Part 7: Save registry
            let app_registry_file = citadel_root.join("apps").join("registry.json");
            let mut app_registry_file =
                std::fs::File::create(app_registry_file).expect("Error opening registry.json!");
            serde_json::to_writer(&mut app_registry_file, &app_registry)
                .expect("Error writing registry.json!");

            let tor_entries_file = citadel_root.join("tor").join("torrc-apps");
            let tor_entries_file_2 = citadel_root.join("tor").join("torrc-apps-2");
            let tor_entries_file_3 = citadel_root.join("tor").join("torrc-apps-3");
            let mut tor_entries_file =
                std::fs::File::create(tor_entries_file).expect("Error opening torrc-apps!");
            let mut tor_entries_file_2 =
                std::fs::File::create(tor_entries_file_2).expect("Error opening torrc-apps-2!");
            let mut tor_entries_file_3 =
                std::fs::File::create(tor_entries_file_3).expect("Error opening torrc-apps-3!");
            // Split entries into 3 groups of the same size
            let mut current_file = 1;

            for entry in tor_entries {
                if current_file == 1 {
                    tor_entries_file
                        .write_all(entry.as_bytes())
                        .expect("Error writing torrc-apps!");
                    current_file = 2;
                } else if current_file == 2 {
                    tor_entries_file_2
                        .write_all(entry.as_bytes())
                        .expect("Error writing torrc-apps-2!");
                    current_file = 3;
                } else if current_file == 3 {
                    tor_entries_file_3
                        .write_all(entry.as_bytes())
                        .expect("Error writing torrc-apps-3!");
                    current_file = 1;
                }
            }
        }
        #[cfg(feature = "dev-tools")]
        SubCommand::Schema { version } => match version.as_str() {
            "3" => {
                let schema = schemars::schema_for!(SchemaItemContainers);
                println!("{}", serde_yaml::to_string(&schema).unwrap());
            }
            "4" => {
                let schema = schemars::schema_for!(AppYml);
                println!("{}", serde_yaml::to_string(&schema).unwrap());
            }
            #[cfg(feature = "umbrel")]
            "umbrel" => {
                let schema = schemars::schema_for!(UmbrelMetadata);
                println!("{}", serde_yaml::to_string(&schema).unwrap());
            }
            "result" => {
                let schema = schemars::schema_for!(ResultYml);
                println!("{}", serde_yaml::to_string(&schema).unwrap());
            }
            "compose" => {
                let schema = schemars::schema_for!(ComposeSpecification);
                println!("{}", serde_yaml::to_string(&schema).unwrap());
            }
            _ => {
                log::error!("Unsupported schema version!");
                exit(1);
            }
        },
        #[cfg(feature = "umbrel")]
        SubCommand::UmbrelToCitadel { app, output } => {
            let app_dir = Path::new(&app);
            let compose_yml = std::fs::File::open(app_dir.join("docker-compose.yml"))
                .expect("Error opening docker-compose.yml!");
            let app_yml = std::fs::File::open(app_dir.join("umbrel-app.yml"))
                .expect("Error opening umbrel-app.yml!");
            let app_yml_parsed: citadel_apps::composegenerator::umbrel::types::Metadata =
                serde_yaml::from_reader(app_yml).expect("Error parsing umbrel-app.yml!");
            let compose_yml_parsed: citadel_apps::composegenerator::compose::types::ComposeSpecification
             = serde_yaml::from_reader(compose_yml).expect("Error parsing docker-compose.yml!");
            let result = citadel_apps::composegenerator::umbrel::convert::convert_compose(
                compose_yml_parsed,
                app_yml_parsed,
            );
            let writer = std::fs::File::create(output).expect("Error creating output file");
            serde_yaml::to_writer(writer, &result).expect("Error saving file!");
        }
        #[cfg(feature = "dev-tools")]
        SubCommand::Validate { app, app_name } => {
            let app_yml = std::fs::File::open(app).expect("Error opening app definition!");
            convert_config(&app_name, &app_yml, &None, &None).expect("App is invalid");
            println!("App is valid!");
        }
        #[cfg(feature = "dev-tools")]
        SubCommand::Update {
            app,
            token,
            include_prerelease,
        } => {
            if let Some(gh_token) = token {
                octocrab::initialise(octocrab::OctocrabBuilder::new().personal_token(gh_token))
                    .expect("Failed to initialise octocrab");
            }
            let path = std::path::Path::new(&app);
            if path.is_file() {
                update_app_yml(path, include_prerelease).await;
            } else if path.is_dir() {
                let app_yml_path = path.join("app.yml");
                if app_yml_path.is_file() {
                    update_app_yml(&app_yml_path, include_prerelease).await;
                } else {
                    let subdirs = std::fs::read_dir(path).expect("Failed to read directory");
                    for subdir in subdirs {
                        let subdir = subdir.unwrap_or_else(|_| {
                            panic!("Failed to read subdir/file in {}", path.display())
                        });
                        let file_type = subdir.file_type().unwrap_or_else(|_| {
                            panic!(
                                "Failed to get filetype of {}/{}",
                                path.display(),
                                subdir.file_name().to_string_lossy()
                            )
                        });
                        if file_type.is_file() {
                            continue;
                        } else if file_type.is_symlink() {
                            eprintln!(
                                "Symlinks like {}/{} are not supported yet!",
                                path.display(),
                                subdir.file_name().to_string_lossy()
                            );
                        } else if file_type.is_dir() {
                            let sub_app_yml = subdir.path().join("app.yml");
                            if sub_app_yml.is_file() {
                                update_app_yml(&sub_app_yml, include_prerelease).await;
                            } else {
                                eprintln!(
                                    "{}/{}/app.yml does not exist or is not a file!",
                                    path.display(),
                                    subdir.file_name().to_string_lossy()
                                );
                                continue;
                            }
                        } else {
                            unreachable!();
                        }
                    }
                }
            } else {
                panic!();
            }
        }
        #[cfg(feature = "dev-tools")]
        SubCommand::V3ToV4 { app } => {
            let app_yml = std::fs::File::open(app.clone()).expect("Error opening app definition!");
            let parsed_app_yml = load_config(app_yml).expect("Failed to parse app.yml");
            match parsed_app_yml {
                citadel_apps::composegenerator::AppYmlFile::V4(_) => {
                    panic!("The app already seems to be an app.yml v4");
                }
                citadel_apps::composegenerator::AppYmlFile::V3(app_yml) => {
                    let writer = std::fs::File::create(app).expect("Error opening app definition!");
                    serde_yaml::to_writer(writer, &v3_to_v4(app_yml, &None))
                        .expect("Error saving app definition!");
                }
            }
            log::info!("App is valid!");
        }
    }
}
