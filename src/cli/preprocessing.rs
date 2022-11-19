use std::{path::Path, io::Read, collections::HashMap};
use crate::composegenerator::compose::types::ComposeSpecification;

use super::{tera, UserJson};

pub fn preprocess_apps(citadel_root: &Path, app_dir: &Path) {
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

    let apps = std::fs::read_dir(app_dir).expect("Error reading apps directory!");
    let apps = apps.filter(|entry| {
        let entry = entry.as_ref().expect("Error reading app directory!");
        let path = entry.path();

        path.is_dir()
    });

    let mut env_vars = Vec::new();

    #[allow(deprecated)]
    if let Ok(dot_env) = dotenv::from_filename_iter(citadel_root.join(".env")) {
        env_vars = dot_env.collect();
    }

    if env_vars.is_empty() && citadel_seed.is_none() {
        eprintln!("Warning: Citadel does not seem to be set up yet!");
    }


    let mut services = Vec::<String>::new();
    let user_json = std::fs::File::open(citadel_root.join("db").join("user.json"));
    if let Ok(user_json) = user_json {
        let user_json = serde_json::from_reader::<_, UserJson>(user_json);
        if let Ok(user_json) = user_json {
            services = user_json.installed_apps;
        }
    }
    services.append(&mut vec!["bitcoind".to_string(), "lnd".to_string()]);

    // Collect the env vars into an hashmap, logging errors
    let env_vars: HashMap<String, String> = env_vars
        .into_iter()
        .filter_map(|result| {
            if let Ok((key, value)) = result {
                Some((key, value))
            } else {
                eprintln!("Warning: Failed to parse env var: {:?}", result);
                None
            }
        })
        .collect();

    for app in apps {
        let app = app.expect("Error reading app directory!");
        let app_id = app.file_name();
        let app_id = app_id.to_str().unwrap();

        if let Err(tera_error) = tera::convert_app_jinja_files(
            &app.path(),
            &services,
            &citadel_seed,
            &Some(env_vars.clone()),
        ) {
            eprintln!("Error converting app jinja files: {:?}", tera_error);
            continue;
        }

        let app_yml = app.path().join("app.yml");
        if !app_yml.exists() {
            #[cfg(feature = "umbrel")]
            {
                let umbrel_app_yml = app.path().join("umbrel-app.yml");
                if umbrel_app_yml.exists() {
                    let compose_yml = std::fs::File::open(app.path().join("docker-compose.yml"))
                        .expect("Error opening docker-compose.yml!");
                    let umbrel_app_yml =
                        std::fs::File::open(umbrel_app_yml).expect("Error opening umbrel-app.yml!");
                    let umbrel_app_yml: crate::composegenerator::umbrel::types::Metadata =
                        serde_yaml::from_reader(umbrel_app_yml)
                            .expect("Error parsing umbrel-app.yml!");
                    let compose_yml_parsed: ComposeSpecification =
                        serde_yaml::from_reader(compose_yml)
                            .expect("Error parsing docker-compose.yml!");
                    let result = crate::composegenerator::umbrel::convert::convert_compose(
                        compose_yml_parsed,
                        umbrel_app_yml,
                    );
                    let writer =
                        std::fs::File::create(&app_yml).expect("Error creating output file");
                    serde_yaml::to_writer(writer, &result).expect("Error saving file!");
                } else {
                    eprintln!("Warning: App {} does not have an app.yml file!", app_id);
                    continue;
                }
            }
            #[cfg(not(feature = "umbrel"))]
            {
                eprintln!("Warning: App {} does not have an app.yml file!", app_id);
                continue;
            }
        }
    }
}