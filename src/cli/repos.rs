use std::{
    collections::{BTreeMap, HashMap},
    fs::File,
    path::Path,
};

use super::{preprocessing::preprocess_apps, UserJson};
use anyhow::Result;
use semver::Version;
use serde::{Deserialize, Serialize};
use tempdir::TempDir;

use crate::{composegenerator::load_config_as_v4, constants::MINIMUM_COMPATIBLE_APP_MANAGER};

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
    icon: String,
    developers: String,
    license: String,

    content: HashMap<String, String>,
    apps: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct AppStoreInfo {
    id: String,
    name: String,
    tagline: String,
    icon: String,
    developers: String,
    license: String,
    apps: HashMap<String, String>,
    commit: String,
    repo: String,
    branch: String,
    subdir: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct AppUpdateInfo {
    id: String,
    new_version: String,
    release_notes: BTreeMap<String, String>,
}

fn get_subdir(app_store: &AppStoreV1) -> Option<String> {
    let mut subdir = None;
    if app_store.content.contains_key(env!("CARGO_PKG_VERSION")) {
        subdir = Some(
            app_store
                .content
                .get(env!("CARGO_PKG_VERSION"))
                .unwrap()
                .clone(),
        );
    } else if app_store
        .content
        .contains_key(&("v".to_owned() + env!("CARGO_PKG_VERSION")))
    {
        subdir = Some(
            app_store
                .content
                .get(&("v".to_owned() + env!("CARGO_PKG_VERSION")))
                .unwrap()
                .clone(),
        );
    } else {
        let current_version = Version::parse(env!("CARGO_PKG_VERSION")).unwrap();
        let minimum_app_manager = Version::parse(MINIMUM_COMPATIBLE_APP_MANAGER).unwrap();
        // The semver of the latest found verion so we can compare it to find a later one
        let mut found_version: Option<Version> = None;
        for (key, value) in app_store.content.iter() {
            let key = key.strip_prefix('v').unwrap_or(key);
            let key = Version::parse(key).unwrap();
            if key >= minimum_app_manager
                && key <= current_version
                && (found_version.is_none() || key > found_version.as_ref().unwrap().clone())
            {
                found_version = Some(key);
                subdir = Some(value.clone());
            }
        }
    }
    subdir
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
                let Some(subdir) = get_subdir(&app_store) else {
                        eprintln!("No compatible version found for {}", source.repo);
                        continue;
                    };
                let mut out_app_store = AppStoreInfo {
                    id: app_store.id,
                    name: app_store.name,
                    tagline: app_store.tagline,
                    icon: app_store.icon,
                    developers: app_store.developers,
                    license: app_store.license,
                    apps: HashMap::new(),
                    commit: git::get_commit(tmp_dir.path())?,
                    repo: source.repo,
                    branch: source.branch,
                    subdir: subdir.clone(),
                };
                let subdir_path = Path::new(&subdir);
                // Copy all dirs from the subdir to the apps dir
                // Overwrite any existing files
                // Skip apps that are already in installed_apps
                let mut store_apps = vec![];
                for entry in std::fs::read_dir(tmp_dir.path().join(subdir_path))? {
                    let entry = entry?;
                    let app_id = entry.file_name().to_str().unwrap().to_string();
                    if installed_apps.contains(&app_id) {
                        eprintln!("App store {} tries to install app {} which is already installed by another store.", out_app_store.id, app_id);
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
                    store_apps.push(app_id);
                }
                out_app_store.apps =
                    git::get_latest_commit_for_apps(tmp_dir.path(), &subdir, &installed_apps)?;
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

pub fn list_updates(citadel_root: &str) -> Result<()> {
    let citadel_root = Path::new(citadel_root);

    let mut services = Vec::<String>::new();
    let user_json = std::fs::File::open(citadel_root.join("db").join("user.json"));
    if let Ok(user_json) = user_json {
        let user_json = serde_json::from_reader::<_, UserJson>(user_json);
        if let Ok(user_json) = user_json {
            services = user_json.installed_apps;
        }
    }
    services.append(&mut vec!["bitcoind".to_string(), "lnd".to_string()]);

    let mut updatable_apps = vec![];

    let stores_yml = citadel_root.join("apps").join("stores.yml");
    let stores_yml = std::fs::File::open(stores_yml)?;
    let stores = serde_yaml::from_reader::<File, Vec<AppStoreInfo>>(stores_yml)?;

    for store in stores {
        let tmp_dir = TempDir::new("citadel")?;
        git::clone(&store.repo, &store.branch, tmp_dir.path())?;
        let commit = git::get_commit(tmp_dir.path())?;
        if commit != store.commit {
            println!("Store {} has an update.", store.id);
            let app_store_yml = tmp_dir.path().join("app-store.yml");
            let app_store_yml = std::fs::File::open(app_store_yml);
            let Ok(app_store_yml) = app_store_yml else {
                eprintln!("No app-store.yml found in {}", store.repo);
                continue;
            };
            let app_store = serde_yaml::from_reader::<File, serde_yaml::Value>(app_store_yml);
            let Ok(app_store) = app_store else {
                eprintln!("Failed to load app-store.yml in {}", store.repo);
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
                        eprintln!("Failed to load app-store.yml in {}", store.repo);
                        continue;
                    };
                    let Some(subdir) = get_subdir(&app_store) else {
                            eprintln!("No compatible version found for {}", store.repo);
                            continue;
                        };
                    let mut all_store_updatable_apps: Vec<String>;
                    if subdir != store.subdir {
                        all_store_updatable_apps = store.apps.clone().into_keys().collect();
                    } else {
                        let latest_commits = git::get_latest_commit_for_apps(
                            tmp_dir.path(),
                            &subdir,
                            &store.apps.clone().into_keys().collect::<Vec<String>>(),
                        );
                        let Ok(mut latest_commits) = latest_commits else {
                            eprintln!("Failed to get latest commits for apps in {}", store.repo);
                            continue;
                        };
                        latest_commits.retain(|app_id, commit| {
                            store.apps.contains_key(app_id)
                                && store.apps.get(app_id).unwrap() != commit
                        });
                        all_store_updatable_apps = latest_commits.into_keys().collect();
                    }
                    let subdir_path = tmp_dir.path().join(subdir);
                    all_store_updatable_apps.retain(|v| subdir_path.join(v).exists());
                    preprocess_apps(citadel_root, &subdir_path);
                    for app_id in all_store_updatable_apps {
                        let app_dir = subdir_path.join(&app_id);
                        let app_yml = app_dir.join("app.yml");
                        let app_yml = std::fs::File::open(app_yml);
                        let Ok(app_yml) = app_yml else {
                            eprintln!("No app.yml found for app {}", app_id);
                            continue;
                        };
                        let app_config = load_config_as_v4(app_yml, &Some(&services));
                        let Ok(app_config) = app_config else {
                            eprintln!("Failed to load app.yml for app {}", app_id);
                            continue;
                        };
                        updatable_apps.push(AppUpdateInfo {
                            id: app_id,
                            new_version: app_config.metadata.version,
                            release_notes: app_config.metadata.release_notes.unwrap_or_default(),
                        })
                    }
                }
                _ => {
                    eprintln!("Unknown app store version: {}", app_store_version);
                    continue;
                }
            }
        }
    }

    let updates_yml = citadel_root.join("apps").join("updates.yml");
    let mut file = File::create(&updates_yml)?;
    serde_yaml::to_writer(&mut file, &updatable_apps)?;

    Ok(())
}

pub fn download_app(citadel_root: &str, app: &str) -> Result<()> {
    let citadel_root = Path::new(citadel_root);
    let stores_yml = citadel_root.join("apps").join("stores.yml");
    let stores_yml = std::fs::File::open(stores_yml)?;
    let stores = serde_yaml::from_reader::<File, Vec<AppStoreInfo>>(stores_yml)?;
    let app_src = stores.iter().find(|store| store.apps.contains_key(app));
    let app_src = app_src.expect("App not found in any store");
    let tmp_dir = TempDir::new("citadel")?;
    git::clone(&app_src.repo, &app_src.branch, tmp_dir.path())?;
    let app_store_yml = tmp_dir.path().join("app-store.yml");
    let app_store_yml = std::fs::File::open(app_store_yml);
    let Ok(app_store_yml) = app_store_yml else {
        eprintln!("No app-store.yml found in {}", app_src.repo);
        return Ok(());
    };
    let app_store = serde_yaml::from_reader::<File, serde_yaml::Value>(app_store_yml);
    let Ok(app_store) = app_store else {
        eprintln!("Failed to load app-store.yml in {}", app_src.repo);
        return Ok(());
    };
    let app_store_version = app_store.get("store_version");
    if app_store_version.is_none() || !app_store_version.unwrap().is_u64() {
        eprintln!("App store version not defined.");
        return Ok(());
    }
    let app_store_version = app_store_version.unwrap().as_u64().unwrap();
    match app_store_version {
        1 => {
            let app_store = serde_yaml::from_value::<AppStoreV1>(app_store);
            let Ok(app_store) = app_store else {
                eprintln!("Failed to load app-store.yml in {}", app_src.repo);
                return Ok(());
            };
            let Some(subdir) = get_subdir(&app_store) else {
                    eprintln!("No compatible version found for {}", app_src.repo);
                    return Ok(());
                };
            // Check if app exists in store
            let app_dir = tmp_dir.path().join(subdir).join(app);
            if !app_dir.exists() {
                eprintln!("App {} not present in {} anymore", app, app_src.repo);
                return Ok(());
            }

            // Overwrite app
            let citadel_app_dir = citadel_root.join("apps").join(app);
            if citadel_app_dir.exists() {
                std::fs::remove_dir_all(&citadel_app_dir)?;
            }
            std::fs::create_dir_all(&citadel_app_dir)?;

            fs_extra::dir::copy(
                &app_dir,
                &citadel_root.join("apps"),
                &fs_extra::dir::CopyOptions {
                    overwrite: true,
                    ..Default::default()
                },
            )?;
        }
        _ => {
            eprintln!("Unknown app store version: {}", app_store_version);
            return Ok(());
        }
    }

    Ok(())
}

pub fn download_new_apps(citadel_root: &str) -> Result<()> {
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
    let citadel_root = Path::new(citadel_root);
    let stores_yml = citadel_root.join("apps").join("stores.yml");
    let stores_yml = std::fs::File::open(stores_yml)?;
    let mut stores = serde_yaml::from_reader::<File, Vec<AppStoreInfo>>(stores_yml)?;
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
                let Some(subdir) = get_subdir(&app_store) else {
                        eprintln!("No compatible version found for {}", source.repo);
                        continue;
                    };
                let mut out_app_store = stores.iter_mut().find(|s| s.repo == source.repo && s.branch == source.branch);
                if out_app_store.is_none() {
                    stores.push(AppStoreInfo {
                        id: app_store.id,
                        name: app_store.name,
                        tagline: app_store.tagline,
                        icon: app_store.icon,
                        developers: app_store.developers,
                        license: app_store.license,
                        apps: HashMap::new(),
                        commit: git::get_commit(tmp_dir.path())?,
                        repo: source.repo.clone(),
                        branch: source.branch.clone(),
                        subdir: subdir.clone(),
                    });
                    out_app_store = stores.iter_mut().find(|s| s.repo == source.repo && s.branch == source.branch);
                };
                let out_app_store = out_app_store.unwrap();
                let subdir_path = Path::new(&subdir);
                // Copy all dirs from the subdir to the apps dir
                // Overwrite any existing files
                // Skip apps that are already in installed_apps
                let mut store_apps = vec![];
                for entry in std::fs::read_dir(tmp_dir.path().join(subdir_path))? {
                    let entry = entry?;
                    let app_id = entry.file_name().to_str().unwrap().to_string();
                    if citadel_root.join("apps").join(&app_id).exists() {
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
                    store_apps.push(app_id);
                }
                let new_apps =
                    git::get_latest_commit_for_apps(tmp_dir.path(), &subdir, &installed_apps)?;
                out_app_store
                    .apps
                    .extend(new_apps.iter().map(|(k, v)| (k.clone(), v.clone())));
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
