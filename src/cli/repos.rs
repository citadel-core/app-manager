use std::{collections::HashMap, fs::File, path::Path};

use anyhow::Result;
use semver::Version;
use serde::{Deserialize, Serialize};
use tempdir::TempDir;

use crate::constants::MINIMUM_COMPATIBLE_APP_MANAGER;

mod git;

#[derive(Debug, Serialize, Deserialize)]
struct AppSrc {
    repo: String,
    branch: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct AppStoreV1 {
    store_version: u8,

    id: String,
    name: String,
    tagline: String,
    developers: String,
    license: String,

    content: HashMap<String, String>,
    apps: Option<Vec<String>>,
}

pub fn download_apps(citadel_root: &str) -> Result<()> {
    let citadel_root = Path::new(citadel_root);
    let sources_yml = citadel_root.join("apps").join("sources.yml");
    if !sources_yml.exists() {
        let default_passwords = vec![AppSrc {
            repo: "https://github.com/citadel-core/apps".to_string(),
            branch: "main".to_string(),
        }];
        let mut file = File::create(&sources_yml)?;
        serde_yaml::to_writer(&mut file, &default_passwords)?;
    }
    let sources_yml = std::fs::File::open(sources_yml)?;
    let sources: Vec<AppSrc> = serde_yaml::from_reader(sources_yml)?;
    let mut installed_apps: Vec<String> = vec![];
    let mut stores = vec![];
    // For each AppSrc, clone the repo into a tempdir
    for source in sources {
        let tmp_dir = TempDir::new("citadel_app")?;
        git::clone(&source.repo, &source.branch, tmp_dir.path())?;
        // Read the app-store.yml, and match the store_version
        let app_store_yml = tmp_dir.path().join("app-store.yml");
        let app_store_yml = std::fs::File::open(app_store_yml);
        let Ok(app_store_yml) = app_store_yml else {
            eprintln!("No app-store.yml found in {}", source.repo);
            continue;
        };
        let app_store = serde_yaml::from_reader::<File, serde_yaml::Value>(app_store_yml);
        let Ok(app_store) = app_store else {
            eprintln!("Failed to load app-store.yml in {}", source.repo);
            continue;
        };
        let app_store_version = app_store.get("store_version");
        if app_store_version.is_none() || !app_store_version.unwrap().is_u64() {
            eprintln!("App store version not defined.");
            continue;
        }
        let app_store_version = app_store_version.unwrap().as_u64().unwrap();
        match app_store_version {
            1 => {
                let app_store = serde_yaml::from_value::<AppStoreV1>(app_store);
                let Ok(app_store) = app_store else {
                    eprintln!("Failed to load app-store.yml in {}", source.repo);
                    continue;
                };
                let mut out_app_store = app_store.clone();
                out_app_store.apps = Some(Vec::new());
                println!(env!("CARGO_PKG_VERSION"));
                let mut subdir = None;
                if app_store.content.contains_key(env!("CARGO_PKG_VERSION")) {
                    subdir = Some(app_store.content.get(env!("CARGO_PKG_VERSION")).unwrap());
                } else if app_store
                    .content
                    .contains_key(&("v".to_owned() + env!("CARGO_PKG_VERSION")))
                {
                    subdir = Some(
                        app_store
                            .content
                            .get(&("v".to_owned() + env!("CARGO_PKG_VERSION")))
                            .unwrap(),
                    );
                } else {
                    let current_version = Version::parse(env!("CARGO_PKG_VERSION")).unwrap();
                    let minimum_app_manager =
                        Version::parse(MINIMUM_COMPATIBLE_APP_MANAGER).unwrap();
                    // The semver of the latest found verion so we can compare it to find a later one
                    let mut found_version: Option<Version> = None;
                    for (key, value) in app_store.content.iter() {
                        if key.starts_with("v") {
                            let key = &key[1..];
                            let key = Version::parse(key).unwrap();
                            if key >= minimum_app_manager
                                && key <= current_version
                                && (found_version.is_none()
                                    || key > found_version.as_ref().unwrap().clone())
                            {
                                found_version = Some(key);
                                subdir = Some(value);
                            }
                        }
                    }
                }
                let Some(subdir) = subdir else {
                        eprintln!("No compatible version found for {}", source.repo);
                        continue;
                    };
                let subdir = Path::new(subdir);
                // Copy all dirs from the subdir to the apps dir
                // Overwrite any existing files
                // Skip apps that are already in installed_apps
                for entry in std::fs::read_dir(tmp_dir.path().join(subdir))? {
                    let entry = entry?;
                    let app_id = entry.file_name().to_str().unwrap().to_string();
                    if installed_apps.contains(&app_id) {
                        eprintln!("App store {} tries to install app {} which is already installed by another store.", source.repo, app_id);
                        continue;
                    }
                    fs_extra::dir::copy(
                        entry.path(),
                        citadel_root.join("apps"),
                        &fs_extra::dir::CopyOptions {
                            overwrite: true,
                            skip_exist: true,
                            buffer_size: 64000,
                            copy_inside: true,
                            depth: 0,
                            content_only: false,
                        },
                    )?;
                    installed_apps.push(app_id.clone());
                    out_app_store.apps.as_mut().unwrap().push(app_id);
                }
                stores.push(out_app_store);
            }
            _ => {
                eprintln!("Unknown app store version: {}", app_store_version);
                continue;
            }
        }
    }

    // Save stores to apps/stores.yml
    let stores_yml = citadel_root.join("apps").join("stores.yml");
    let mut file = File::create(&stores_yml)?;
    serde_yaml::to_writer(&mut file, &stores)?;

    Ok(())
}
