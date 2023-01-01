use std::collections::HashMap;

use serde_json::{Map, Value};

use super::permissions;
use super::types::PortMapElement;
use crate::composegenerator::compose::types::Command;
use crate::utils::find_env_vars;
use anyhow::{bail, Result};
use hex;
use hmac_sha256::HMAC;
use serde_json::Value::Object;

pub fn derive_entropy(seed: &str, identifier: &str) -> String {
    let mut hasher = HMAC::new(seed);
    hasher.update(identifier);
    let result = hasher.finalize();
    hex::encode(result)
}

pub fn validate_cmd(
    app_name: &str,
    command: &Command,
    permissions: &[&String],
) -> Result<()> {
    match command {
        Command::SimpleCommand(simple_command) => {
            let env_vars = find_env_vars(simple_command);
            for env_var in env_vars {
                if !permissions::is_allowed_by_permissions(app_name, env_var, permissions) {
                    bail!("Env var {} not allowed by permissions", env_var);
                }
            }
        }
        Command::ArrayCommand(values) => {
            for value in values {
                let env_vars = find_env_vars(value);
                for env_var in env_vars {
                    if !permissions::is_allowed_by_permissions(app_name, env_var, permissions) {
                        bail!("Env var {} not allowed by permissions", env_var);
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn get_host_port(port_map: &[PortMapElement], internal_port: u16) -> Option<&PortMapElement> {
    return port_map
        .iter()
        .find(|&elem| elem.internal_port == internal_port);
}

pub fn validate_port_map_app(
    port_map_app: &Map<String, Value>,
) -> Result<HashMap<String, Vec<PortMapElement>>> {
    Ok(serde_json::from_value::<
        HashMap<String, Vec<PortMapElement>>,
    >(Object(port_map_app.to_owned()))?)
}

pub fn get_main_container(services: &HashMap<String, super::types::Container>) -> Result<&str> {
    if services.len() == 1 {
        return Ok(services.keys().next().unwrap());
    }

    let mut main_service_name: Option<&String> = Option::<&String>::None;
    // We now have a list of services whose dependencies are present
    // And that are mostly validated
    // We can now determine the main container of the app
    for service_name in services.keys() {
        // web is for easier porting from Umbrel and to preserve compatibility with v3
        if service_name == "main" || service_name == "web" {
            main_service_name = Some(service_name);
            break;
        } else if service_name.starts_with("main") {
            if main_service_name.is_some() {
                tracing::error!(
                    "Container {} and {} could both be main container",
                    service_name,
                    main_service_name.unwrap()
                );
                bail!("Multiple main containers in app!");
            }
            main_service_name = Some(service_name);
        }
    }
    if let Some(main_name) = main_service_name {
        Ok(main_name)
    } else {
        bail!("No main container found!")
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn validate_port_map_app() {
        let example_port_map = json!({
            "main": [
                {
                    "internalPort": 3000,
                    "publicPort": 3000,
                    "dynamic": true,
                }
            ]
        });
        let result = super::validate_port_map_app(example_port_map.as_object().unwrap());
        assert!(result.is_ok());
    }

    #[test]
    fn derive_entropy() {
        let result = super::derive_entropy("seed", "identifier");
        assert_eq!(
            result,
            "30d473de86ac35de605cc672766d3918c568fcc2df05d4f122a0b2a110d12e39"
        );
    }
}
