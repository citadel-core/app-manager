use std::{collections::HashMap, io::Read, path::Path};

use super::{tera, umbrel::convert, UserJson};

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
    services.append(&mut vec!["bitcoind".to_string()]);

    for app in apps {
        let app = app.expect("Error reading app directory!");
        let app_id = app.file_name();
        let app_id = app_id.to_str().unwrap();

        if let Err(tera_error) = tera::convert_app_yml(
            &app.path(),
            &services,
            &citadel_seed,
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
                    if let Err(convert_error) = convert(&app.path()) {
                        eprintln!(
                            "Error converting Umbrel app to Citadel app: {:?}",
                            convert_error
                        );
                        continue;
                    }
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

pub fn preprocess_config_files(citadel_root: &Path, app_dir: &Path) {
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
    services.append(&mut vec!["bitcoind".to_string()]);

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

        if let Err(tera_error) = tera::convert_app_config_files(
            &app.path(),
            &services,
            &citadel_seed,
            &Some(env_vars.clone()),
        ) {
            eprintln!("Error converting app jinja files: {:?}", tera_error);
            continue;
        }
    }
}
