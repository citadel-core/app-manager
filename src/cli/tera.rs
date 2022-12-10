use std::{
    collections::HashMap,
    io::{Read, Write},
    path::Path,
};

use rand::RngCore;
use tera::{renderer::processor::Processor, Tera};

use crate::{
    composegenerator::{
        load_config_as_v4,
        v4::{
            permissions::{is_allowed_by_permissions, ALWAYS_ALLOWED_ENV_VARS},
            utils::{derive_entropy, get_main_container},
        },
    },
    utils::flatten,
};

use anyhow::{bail, Result};
use sha1::Digest;

// Creates a S2K hash like used by Tor
fn tor_hash(input: &str, salt: [u8; 8]) -> String {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&salt);
    bytes.extend_from_slice(input.as_bytes());
    let mut hash = sha1::Sha1::new();
    while bytes.len() < 0x10000 {
        bytes.extend_from_slice(&salt);
        bytes.extend_from_slice(input.as_bytes());
    }
    bytes.truncate(0x10000);
    hash.update(&bytes);
    let hash = hash.finalize();
    let mut hash_str = String::new();
    for byte in hash {
        hash_str.push_str(&format!("{:02X}", byte));
    }
    format!(
        "16:{}60{}",
        hex::encode(salt).to_uppercase(),
        hash_str.to_uppercase()
    )
}

fn random_hex_string(len: usize) -> String {
    let mut rng = rand::thread_rng();
    let mut bytes = vec![0u8; len];
    rng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

pub fn convert_app_yml(
    app_path: &Path,
    services: &[String],
    env_vars: &HashMap<String, String>,
    citadel_seed: &Option<String>,
) -> Result<()> {
    let app_yml_jinja = app_path.to_path_buf().join("app.yml.jinja");
    if app_yml_jinja.exists() && citadel_seed.is_some() {
        convert_app_yml_internal(
            &app_yml_jinja,
            app_path.file_name().unwrap().to_str().unwrap(),
            services,
            env_vars,
            citadel_seed.as_ref().unwrap(),
        )?;
    }
    Ok(())
}

fn convert_app_yml_internal(
    jinja_file: &Path,
    app_id: &str,
    services: &[String],
    env_vars: &HashMap<String, String>,
    citadel_seed: &str,
) -> Result<()> {
    let mut context = tera::Context::new();
    context.insert("services", services);
    context.insert("app_name", app_id);
    let mut tmpl = String::new();
    std::fs::File::open(jinja_file)?.read_to_string(&mut tmpl)?;
    let mut tera = Tera::default();
    let citadel_seed = citadel_seed.to_string();
    let app_id = app_id.to_string();
    for (key, val) in env_vars {
        // We can't know the permissions at this stage, so we only allow the env vars here that are always allowed
        if ALWAYS_ALLOWED_ENV_VARS.contains(&key.as_str()) {
            context.insert(key, &val);
        }
    }
    tera.register_function(
        "derive_entropy",
        move |args: &HashMap<String, serde_json::Value>| -> Result<tera::Value, tera::Error> {
            let identifier = if args.contains_key("id") {
                args.get("id")
            } else {
                args.get("identifier")
            };
            let Some(identifier) = identifier else {
                return Err(tera::Error::msg("Missing identifier"));
            };

            let Some(identifier) = identifier.as_str() else {
                return Err(tera::Error::msg("Identifier must be a string"));
            };

            Ok(tera::to_value(derive_entropy(
                &citadel_seed,
                format!("app-{}-{}", app_id.replace('-', "_"), identifier).as_str(),
            ))
            .expect("Failed to serialize value"))
        },
    );
    tera.register_filter(
        "tor_hash",
        |val: &tera::Value,
         _args: &HashMap<String, tera::Value>|
         -> Result<tera::Value, tera::Error> {
            let Some(input) = val.as_str() else {
            return Err(tera::Error::msg("Identifier must be a string"));
        };
            let mut salt = [0u8; 8];
            rand::thread_rng().fill_bytes(&mut salt);
            Ok(tera::to_value(tor_hash(input, salt)).expect("Failed to serialize value"))
        },
    );
    tera.register_function(
        "gen_seed",
        |args: &HashMap<String, tera::Value>| -> Result<tera::Value, tera::Error> {
            let Some(len) = args.get("len") else {
                return Err(tera::Error::msg("Length must be defined"));
            };
            let Some(len) = len.as_u64() else {
                return Err(tera::Error::msg("Length must be a number"));
            };
            Ok(tera::to_value(random_hex_string(len as usize)).expect("Failed to serialize value"))
        },
    );
    let tmpl_result = tera.render_str(tmpl.as_str(), &context);
    if let Err(e) = tmpl_result {
        bail!("Error processing template {}: {}", jinja_file.display(), e);
    }
    let mut writer = std::fs::File::create(jinja_file.to_path_buf().with_extension(""))?;
    writer.write_all(tmpl_result.unwrap().as_bytes())?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn generate_tera<'a>(
    app_id: &str,
    app_version: &str,
    permissions: &[&'a String],
    services: &[String],
    services_with_hs: &[&String],
    env_vars: &HashMap<String, String>,
    citadel_seed: &str,
    tor_dir: &Path,
) -> Result<(Tera, tera::Context)> {
    let mut context = tera::Context::new();
    context.insert("services", &services);
    context.insert("app_name", app_id);

    for (key, val) in env_vars {
        if is_allowed_by_permissions(app_id, key, permissions) {
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
            &derive_entropy(citadel_seed, format!("app-{}-seed{}", app_id, i).as_str()),
        );
    }
    context.insert("APP_VERSION", app_version);

    if tor_dir.is_dir() {
        let subdirs = std::fs::read_dir(tor_dir)?.filter_map(|dir| {
            if let Ok(dir) = dir {
                let path = dir.path();
                if path.is_dir() {
                    Some(path)
                } else {
                    None
                }
            } else {
                None
            }
        });
        let app_name = format!("app-{}", app_id);
        let app_prefix = format!("app-{}-", app_id);
        for dir in subdirs {
            let dir_name = dir.file_name().unwrap().to_str().unwrap();
            if dir_name != app_name && !dir_name.starts_with(&app_prefix) {
                continue;
            }
            let hostname_file = dir.join("hostname");
            let mut hostname = "notyetgenerated.onion".to_string();
            if hostname_file.exists() {
                let mut hostname_file = std::fs::File::open(hostname_file)?;
                hostname_file.read_to_string(&mut hostname)?;
            }
            context.insert(
                if dir_name == app_name {
                    "APP_HIDDEN_SERVICE".to_string()
                } else {
                    format!(
                        "APP_HIDDEN_SERVICE_{}",
                        &dir_name[app_prefix.len()..]
                            .to_uppercase()
                            .replace('-', "_")
                    )
                },
                &hostname.trim(),
            );
        }
    } else {
        context.insert("APP_HIDDEN_SERVICE", "notyetgenerated.onion");
    }
    for service in services_with_hs {
        let key = format!(
            "APP_HIDDEN_SERVICE_{}",
            service.to_uppercase().replace('-', "_")
        );
        if context.get(&key).is_none() {
            context.insert(key, "notyetgenerated.onion");
        }
    }
    let mut tera = Tera::default();
    let citadel_seed = citadel_seed.to_string();
    let app_id = app_id.to_string();
    tera.register_function(
        "derive_entropy",
        move |args: &HashMap<String, serde_json::Value>| -> Result<tera::Value, tera::Error> {
            let identifier = if args.contains_key("id") {
                args.get("id")
            } else {
                args.get("identifier")
            };
            let Some(identifier) = identifier else {
                return Err(tera::Error::msg("Missing identifier"));
            };

            let Some(identifier) = identifier.as_str() else {
                return Err(tera::Error::msg("Identifier must be a string"));
            };

            Ok(tera::to_value(derive_entropy(
                &citadel_seed,
                format!("app-{}-{}", app_id.replace('-', "_"), identifier).as_str(),
            ))
            .expect("Failed to serialize value"))
        },
    );
    tera.register_filter(
        "tor_hash",
        |val: &tera::Value,
         _args: &HashMap<String, tera::Value>|
         -> Result<tera::Value, tera::Error> {
            let Some(input) = val.as_str() else {
            return Err(tera::Error::msg("Identifier must be a string"));
        };
            let mut salt = [0u8; 8];
            rand::thread_rng().fill_bytes(&mut salt);
            Ok(tera::to_value(tor_hash(input, salt)).expect("Failed to serialize value"))
        },
    );
    tera.register_function(
        "gen_seed",
        |args: &HashMap<String, tera::Value>| -> Result<tera::Value, tera::Error> {
            let Some(len) = args.get("len") else {
                return Err(tera::Error::msg("Length must be defined"));
            };
            let Some(len) = len.as_u64() else {
                return Err(tera::Error::msg("Length must be a number"));
            };
            Ok(tera::to_value(random_hex_string(len as usize)).expect("Failed to serialize value"))
        },
    );
    Ok((tera, context))
}

pub fn convert_app_config_files(
    app_path: &Path,
    services: &[String],
    citadel_seed: &Option<String>,
    env_vars: &Option<HashMap<String, String>>,
    tor_dir: &Path,
) -> Result<()> {
    if citadel_seed.is_some() && env_vars.is_some() {
        let citadel_seed = citadel_seed.as_ref().unwrap();
        let env_vars = env_vars.as_ref().unwrap();

        let app_yml = app_path.join("app.yml");
        if !app_yml.exists() {
            bail!("app.yml not found in {}", app_path.display());
        }
        let app_yml = std::fs::File::open(app_yml)?;
        let app_yml = load_config_as_v4(app_yml, &Some(&services.to_vec()));
        if let Err(e) = app_yml {
            bail!(
                "Error processing app.yml {}: {}",
                app_path.join("app.yml").display(),
                e
            );
        }
        let app_yml = app_yml.unwrap();
        let app_version = app_yml.metadata.version;
        let perms = flatten(&app_yml.metadata.permissions);

        let other_jinja_files = std::fs::read_dir(app_path)?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().unwrap_or_default() == "jinja")
            .map(|entry| entry.path());

        let main_container = get_main_container(&app_yml.services)?;
        let services_with_hs = app_yml.services.iter().filter_map(|(name, service)| {
            if name == main_container || service.hidden_services.is_none() {
                None
            } else {
                Some((name, service.hidden_services.as_ref().unwrap()))
            }
        });
        let mut existing_hs = Vec::new();
        for (container, hs) in services_with_hs {
            match hs {
                crate::composegenerator::v4::types::HiddenServices::PortMap(_) => {
                    existing_hs.push(container)
                }
                crate::composegenerator::v4::types::HiddenServices::LayeredMap(map) => {
                    let mut keys = map.keys().collect();
                    existing_hs.append(&mut keys)
                }
            }
        }

        let (mut tera, mut context) = generate_tera(
            app_path.file_name().unwrap().to_str().unwrap(),
            &app_version,
            &perms,
            services,
            &existing_hs,
            env_vars,
            citadel_seed,
            tor_dir,
        )?;

        // Sort other_jinja_files alphabetically so that we can process them in a deterministic order
        // But files called _vars.jinja must be processed first
        let mut other_jinja_files: Vec<_> = other_jinja_files.collect();
        other_jinja_files.sort();
        other_jinja_files.sort_by_key(|path| {
            if path.file_name().unwrap() == "_vars.jinja" {
                0
            } else {
                1
            }
        });
        for jinja_file in other_jinja_files {
            if jinja_file.file_name().unwrap() == "_vars.jinja" {
                let mut file = std::fs::File::open(&jinja_file)?;
                let mut tmpl = String::new();
                file.read_to_string(&mut tmpl)?;
                tera.add_raw_template("_vars", &tmpl)?;
                let mut output = Vec::with_capacity(2000);
                let tmpl = tera.get_template("_vars")?;
                let mut processor =
                    Processor::new(tmpl, &tera, &context, false);
                processor.render(&mut output)?;
                // We ignore the output for this file
                let ctx = processor.get_ctx();
                for (key, value) in ctx {
                    context.insert(key, &value);
                }
            } else {
                let output_file = jinja_file.with_extension("");
                let mut file = std::fs::File::open(&jinja_file)?;
                let mut tmpl = String::new();
                file.read_to_string(&mut tmpl)?;
                let tmpl_result = tera.render_str(tmpl.as_str(), &context);
                if let Err(e) = tmpl_result {
                    bail!("Error processing template {}: {}", jinja_file.display(), e);
                }
                let mut writer = std::fs::File::create(output_file)?;
                writer.write_all(tmpl_result.unwrap().as_bytes())?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use super::tor_hash;

    #[test]
    fn hash_matches_tor() {
        assert_eq!(
            tor_hash("test123", [0x3E, 0x6B, 0xF3, 0xDC, 0xEC, 0x50, 0xFE, 0x51]),
            "16:3E6BF3DCEC50FE5160DBD0C3A9132DB0118AFA5104FE8DA29ADC20A65E"
        );
    }
}
