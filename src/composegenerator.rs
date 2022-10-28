pub mod compose;
pub mod types;
#[cfg(feature = "umbrel")]
pub mod umbrel;
pub mod v3;
pub mod v4;
// A subset of compose
pub mod output;

use std::collections::HashMap;

use self::types::ResultYml;
use self::v3::convert::v3_to_v4;
use self::v3::types::Schema as AppYmlV3;
use self::v4::types::{AppYml as AppYmlV4, PortMapElement};

pub enum AppYmlFile {
    V3(AppYmlV3),
    V4(AppYmlV4),
}

pub fn load_config<R>(app_reader: R) -> Result<AppYmlFile, String>
where
    R: std::io::Read,
{
    let app_yml = serde_yaml::from_reader::<R, serde_yaml::Value>(app_reader)
        .expect("Failed to parse app.yml");
    if !app_yml.is_mapping() {
        return Err("App.yml is not a map!".to_string());
    }
    let version: u64;
    if app_yml.get("citadel_version").is_none()
        || !app_yml.get("citadel_version").unwrap().is_number()
    {
        if app_yml.get("version").is_some() && app_yml.get("version").unwrap().is_number() {
            version = app_yml.get("version").unwrap().as_u64().unwrap();
        } else {
            return Err("Citadel file format is not set or not a number!".to_string());
        }
    } else {
        version = app_yml.get("citadel_version").unwrap().as_u64().unwrap();
    }
    match version {
        3 => {
            let app_definition: Result<AppYmlV3, serde_yaml::Error> =
                serde_yaml::from_value(app_yml);
            match app_definition {
                Ok(app_definition) => Ok(AppYmlFile::V3(app_definition)),
                Err(error) => Err(format!("Error loading app.yml as v3: {}", error)),
            }
        }
        4 => {
            let app_definition: Result<AppYmlV4, serde_yaml::Error> =
                serde_yaml::from_value(app_yml);
            match app_definition {
                Ok(app_definition) => Ok(AppYmlFile::V4(app_definition)),
                Err(error) => Err(format!("Error loading app.yml as v4: {}", error)),
            }
        }
        _ => Err("Version not supported".to_string()),
    }
}

pub fn load_config_as_v4<R>(
    app_reader: R,
    installed_services: &Option<&Vec<String>>,
) -> Result<AppYmlV4, String>
where
    R: std::io::Read,
{
    let app_yml = serde_yaml::from_reader::<R, serde_yaml::Value>(app_reader)
        .expect("Failed to parse app.yml");
    if !app_yml.is_mapping() {
        return Err("App.yml is not a map!".to_string());
    }
    let version: u64;
    if app_yml.get("citadel_version").is_none()
        || !app_yml.get("citadel_version").unwrap().is_number()
    {
        if app_yml.get("version").is_some() && app_yml.get("version").unwrap().is_number() {
            version = app_yml.get("version").unwrap().as_u64().unwrap();
        } else {
            return Err("Citadel file format is not set or not a number!".to_string());
        }
    } else {
        version = app_yml.get("citadel_version").unwrap().as_u64().unwrap();
    }
    match version {
        3 => {
            let app_definition: Result<AppYmlV3, serde_yaml::Error> =
                serde_yaml::from_value(app_yml);
            match app_definition {
                Ok(app_definition) => Ok(v3_to_v4(app_definition, installed_services)),
                Err(error) => Err(format!("Error loading app.yml as v3: {}", error)),
            }
        }
        4 => {
            let app_definition: Result<AppYmlV4, serde_yaml::Error> =
                serde_yaml::from_value(app_yml);
            match app_definition {
                Ok(app_definition) => Ok(app_definition),
                Err(error) => Err(format!("Error loading app.yml as v4: {}", error)),
            }
        }
        _ => Err("Version not supported".to_string()),
    }
}

pub fn convert_config<R>(
    app_name: &str,
    app_reader: R,
    port_map: &Option<HashMap<String, HashMap<String, Vec<PortMapElement>>>>,
    installed_services: &Option<Vec<String>>,
) -> Result<ResultYml, String>
where
    R: std::io::Read,
{
    let app_yml = load_config(app_reader).expect("Failed to parse app.yml");
    match app_yml {
        AppYmlFile::V4(app_definition) => {
            v4::convert::convert_config(app_name, app_definition, port_map, installed_services)
        }
        AppYmlFile::V3(app_definition) => {
            if let Some(installed_services) = installed_services {
                v3::convert::convert_config(app_name, app_definition, port_map, installed_services)
            } else {
                Err("No installed services defined. If you are trying to validate an app, please make sure it is an app.yml v4 or later.".to_string())
            }
        }
    }
}
