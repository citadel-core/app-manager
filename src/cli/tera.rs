use std::{
    collections::HashMap,
    io::{Error, Read, Write},
    path::Path,
};

use rand::RngCore;
use tera::Tera;

use crate::{
    composegenerator::{
        load_config_as_v4,
        v4::{permissions::is_allowed_by_permissions, utils::derive_entropy},
    },
    utils::flatten,
};

use anyhow::Result;
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

pub fn convert_app_yml(
    app_path: &Path,
    services: &[String],
    citadel_seed: &Option<String>,
) -> Result<()> {
    let app_yml_jinja = app_path.to_path_buf().join("app.yml.jinja");
    if app_yml_jinja.exists() && citadel_seed.is_some() {
        convert_app_yml_internal(
            &app_yml_jinja,
            app_path.file_name().unwrap().to_str().unwrap(),
            services,
            citadel_seed.as_ref().unwrap(),
        )?;
    }
    Ok(())
}

fn convert_app_yml_internal(
    jinja_file: &Path,
    app_id: &str,
    services: &[String],
    citadel_seed: &str,
) -> Result<(), Error> {
    let mut context = tera::Context::new();
    context.insert("services", services);
    context.insert("app_name", app_id);
    let mut tmpl = String::new();
    std::fs::File::open(jinja_file)?.read_to_string(&mut tmpl)?;
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
    let tmpl_result = tera.render_str(tmpl.as_str(), &context);
    if let Err(e) = tmpl_result {
        eprintln!("Error processing template: {}", e);
        return Err(Error::new(
            std::io::ErrorKind::Other,
            "Error parsing template",
        ));
    }
    let mut writer = std::fs::File::create(jinja_file.to_path_buf().with_extension(""))?;
    writer.write_all(tmpl_result.unwrap().as_bytes())
}

fn convert_config_template(
    jinja_file: &Path,
    app_id: &str,
    app_version: &str,
    permissions: &[String],
    services: &[String],
    env_vars: &HashMap<String, String>,
    citadel_seed: &str,
) -> Result<(), Error> {
    let output_file = jinja_file.with_extension("");
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

    let mut tmpl = String::new();
    std::fs::File::open(jinja_file)?.read_to_string(&mut tmpl)?;
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
    let tmpl_result = tera.render_str(tmpl.as_str(), &context);
    if let Err(e) = tmpl_result {
        eprintln!("Error processing template: {}", e);
        return Err(Error::new(
            std::io::ErrorKind::Other,
            "Error parsing template",
        ));
    }
    let mut writer = std::fs::File::create(output_file)?;
    writer.write_all(tmpl_result.unwrap().as_bytes())
}

pub fn convert_app_config_files(
    app_path: &Path,
    services: &[String],
    citadel_seed: &Option<String>,
    env_vars: &Option<HashMap<String, String>>,
) -> Result<(), Error> {
    if citadel_seed.is_some() && env_vars.is_some() {
        let citadel_seed = citadel_seed.as_ref().unwrap();
        let env_vars = env_vars.as_ref().unwrap();

        let app_yml = app_path.join("app.yml");
        if !app_yml.exists() {
            return Err(Error::new(std::io::ErrorKind::Other, "app.yml not found"));
        }
        let app_yml = std::fs::File::open(app_yml)?;
        let app_yml = load_config_as_v4(app_yml, &Some(&services.to_vec()));
        if let Err(e) = app_yml {
            eprintln!("Error processing app.yml: {}", e);
            return Err(Error::new(
                std::io::ErrorKind::Other,
                "Error parsing app.yml",
            ));
        }
        let app_yml = app_yml.unwrap();
        let app_version = app_yml.metadata.version;
        let perms = flatten(app_yml.metadata.permissions);

        let other_jinja_files = std::fs::read_dir(app_path)?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().unwrap_or_default() == "jinja")
            .map(|entry| entry.path());

        for jinja_file in other_jinja_files {
            convert_config_template(
                &jinja_file,
                app_path.file_name().unwrap().to_str().unwrap(),
                &app_version,
                &perms,
                services,
                env_vars,
                citadel_seed,
            )?;
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
