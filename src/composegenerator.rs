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
use anyhow::{bail, Result};

pub enum AppYmlFile {
    V3(AppYmlV3),
    V4(AppYmlV4),
}

pub fn load_config<R>(app_reader: R) -> Result<AppYmlFile>
where
    R: std::io::Read,
{
    let app_yml = serde_yaml::from_reader::<R, serde_yaml::Value>(app_reader)?;
    if !app_yml.is_mapping() {
        bail!("App.yml is not a map!");
    }
    let version: u64;
    if app_yml.get("citadel_version").is_none()
        || !app_yml.get("citadel_version").unwrap().is_u64()
    {
        if app_yml.get("version").is_some() && app_yml.get("version").unwrap().is_u64() {
            version = app_yml.get("version").unwrap().as_u64().unwrap();
        } else {
            bail!("Citadel file format is not set or not a number!");
        }
    } else {
        version = app_yml.get("citadel_version").unwrap().as_u64().unwrap();
    }
    match version {
        3 => {
            let app_definition: AppYmlV3 = serde_yaml::from_value(app_yml)?;
            Ok(AppYmlFile::V3(app_definition))
        }
        4 => {
            let app_definition: AppYmlV4 = serde_yaml::from_value(app_yml)?;
            Ok(AppYmlFile::V4(app_definition))
        }
        _ => bail!("Version {} of app.yml not supported", version),
    }
}

pub fn load_config_as_v4<R>(
    app_reader: R,
    installed_services: &Option<&Vec<String>>,
) -> Result<AppYmlV4>
where
    R: std::io::Read,
{
    let app_yml = serde_yaml::from_reader::<R, serde_yaml::Value>(app_reader)?;
    if !app_yml.is_mapping() {
        bail!("App.yml is not a map!");
    }
    let version: u64;
    if app_yml.get("citadel_version").is_none()
        || !app_yml.get("citadel_version").unwrap().is_u64()
    {
        if app_yml.get("version").is_some() && app_yml.get("version").unwrap().is_u64() {
            version = app_yml.get("version").unwrap().as_u64().unwrap();
        } else {
            bail!("Citadel file format is not set or not a number!");
        }
    } else {
        version = app_yml.get("citadel_version").unwrap().as_u64().unwrap();
    }
    match version {
        3 => {
            let app_definition: AppYmlV3 = serde_yaml::from_value(app_yml)?;
            Ok(v3_to_v4(app_definition, installed_services))
        }
        4 => {
            let app_definition: AppYmlV4 = serde_yaml::from_value(app_yml)?;
            Ok(app_definition)
        }
        _ => bail!("Version {} of app.yml not supported", version),
    }
}

pub fn convert_config<R>(
    app_name: &str,
    app_reader: R,
    port_map: &Option<HashMap<String, HashMap<String, Vec<PortMapElement>>>>,
    installed_services: &Option<Vec<String>>,
    ip_addresses: &Option<HashMap<String, String>>,
) -> Result<ResultYml>
where
    R: std::io::Read,
{
    let app_yml = load_config(app_reader)?;
    match app_yml {
        AppYmlFile::V4(app_definition) => v4::convert::convert_config(
            app_name,
            app_definition,
            port_map,
            installed_services,
            ip_addresses,
        ),
        AppYmlFile::V3(app_definition) => {
            if let Some(installed_services) = installed_services {
                v3::convert::convert_config(
                    app_name,
                    app_definition,
                    port_map,
                    installed_services,
                    ip_addresses,
                )
            } else {
                bail!("No installed services defined. If you are trying to validate an app, please make sure it is an app.yml v4 or later.")
            }
        }
    }
}
