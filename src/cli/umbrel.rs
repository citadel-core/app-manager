// This file is pure chaos, but it works (for most apps)

use anyhow::{bail, Result};
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;

use crate::composegenerator::compose::types::ComposeSpecification;
use crate::composegenerator::umbrel::convert::convert_compose;
use crate::composegenerator::umbrel::types::Metadata;
use crate::conch::lexer::Lexer;
use crate::conch::parse::DefaultParser;

use lazy_static::lazy_static;

lazy_static! {
    // Matches ENV_VAR_NAME=
    static ref ENV_VAR_REGEX: Regex = Regex::new(r"\w+=$").unwrap();
}

enum LocalEnvVar {
    Literal(String),
    TorDataDir(String),
}

const ALLOWED_KEYS_FOR_UMBREL_APPS: [&str; 18] = [
    "image",
    "user",
    "stop_grace_period",
    "stop_signal",
    "depends_on",
    "network_mode",
    "restart",
    "init",
    "extra_hosts",
    "entrypoint",
    "working_dir",
    "command",
    "environment",
    "cap_add",
    "volumes",
    "networks",
    "ports",
    // Ignored right now, TODO: Add to app.yml
    "healthcheck",
];

/// Takes a directory that contains an Umbrel app and check if it can run on Citadel, if possible, port it to Citadel
/// The app.yml will be written to the same directory
/// The result will indicate success or failure
pub fn convert(dir: &Path) -> Result<()> {
    let umbrel_app_yml = std::fs::File::open(dir.join("umbrel-app.yml"))?;
    let umbrel_app_yml = serde_yaml::from_reader(umbrel_app_yml)?;
    let metadata: Metadata = serde_yaml::from_value(umbrel_app_yml)?;
    let compose_yml = std::fs::File::open(dir.join("docker-compose.yml"))?;
    let compose_yml: serde_yaml::Value = serde_yaml::from_reader(compose_yml)?;
    let binding = compose_yml.clone();
    let services = binding
        .get("services")
        .ok_or_else(|| anyhow::Error::msg("No services found"))?
        .as_mapping()
        .ok_or_else(|| anyhow::Error::msg("Services is not a mapping"))?;
    for (_, service) in services {
        let keys = service.as_mapping().unwrap().keys();
        let unsupported_keys: _ = keys
            .filter(|k| !ALLOWED_KEYS_FOR_UMBREL_APPS.contains(&k.as_str().unwrap()))
            .collect::<Vec<&serde_yaml::Value>>();
        if !unsupported_keys.is_empty() {
            // Workaround for some apps that are implemented badly
            if unsupported_keys.len() == 1
                && (((metadata.id == "calibre-web" || metadata.id == "sphinx-relay")
                    && unsupported_keys[0].as_str().unwrap() == "container_name")
                    || ((metadata.id == "syncthing")
                        && unsupported_keys[0].as_str().unwrap() == "hostname"))
            {
                // Ignore
                continue;
            }
            tracing::error!(
                "Unsupported keys in docker-compose.yml: {:?}",
                unsupported_keys
            );
            bail!("Unsupported key in docker-compose.yml");
        }
    }
    let compose_yml = serde_yaml::from_value::<ComposeSpecification>(compose_yml)?;

    let mut env_vars = HashMap::<String, String>::new();
    let exports_sh = dir.join("exports.sh");
    if exports_sh.exists() {
        let exports_sh = std::fs::read_to_string(exports_sh)?;
        let mut local_env_vars = HashMap::new();

        let lexer = Lexer::new(exports_sh.chars());
        let parser = DefaultParser::new(lexer);
        for t in parser {
            if let Err(e) = t {
                eprintln!("Error parsing exports.sh: {e}");
                break;
            }
            let t = t.unwrap();
            let t = t.0;
            let mut is_env_var_decl = false;
            let mut declares_env_var = None;
            let mut env_var_value = None;
            match t {
                crate::conch::ast::Command::Job(_) => bail!("Not implemented yet"),
                crate::conch::ast::Command::List(list) => {
                    if !list.rest.is_empty() {
                        bail!("Not implemented yet");
                    }
                    match list.first {
                        crate::conch::ast::ListableCommand::Pipe(_, _) => {
                            bail!("Not implemented yet")
                        }
                        crate::conch::ast::ListableCommand::Single(cmd) => match cmd {
                            crate::conch::ast::PipeableCommand::Simple(cmd) => {
                                if !cmd.redirects_or_env_vars.is_empty() {
                                    for thing in cmd.redirects_or_env_vars {
                                        match thing {
                                            crate::conch::ast::RedirectOrEnvVar::Redirect(_) => {
                                                bail!("Not implemented yet")
                                            }
                                            crate::conch::ast::RedirectOrEnvVar::EnvVar(
                                                name,
                                                val,
                                            ) => {
                                                if val.is_none() {
                                                    eprintln!("Error parsing exports.sh: env var {name} has no value");
                                                    continue;
                                                }
                                                match val.unwrap().0 {
                                                        crate::conch::ast::ComplexWord::Concat(_) => bail!("Not implemented yet"),
                                                        crate::conch::ast::ComplexWord::Single(single) => {
                                                            match single {
                                                                crate::conch::ast::Word::Simple(simple) => {
                                                                    match simple {
                                                                        crate::conch::ast::SimpleWord::Literal(lit) => {
                                                                            local_env_vars.insert(name, LocalEnvVar::Literal(lit));
                                                                        },
                                                                        crate::conch::ast::SimpleWord::Escaped(_) => bail!("Not implemented yet"),
                                                                        crate::conch::ast::SimpleWord::Param(_) => bail!("Not implemented yet"),
                                                                        crate::conch::ast::SimpleWord::Subst(_) => bail!("Not implemented yet"),
                                                                        crate::conch::ast::SimpleWord::Star => bail!("Not implemented yet"),
                                                                        crate::conch::ast::SimpleWord::Question => bail!("Not implemented yet"),
                                                                        crate::conch::ast::SimpleWord::SquareOpen => bail!("Not implemented yet"),
                                                                        crate::conch::ast::SimpleWord::SquareClose => bail!("Not implemented yet"),
                                                                        crate::conch::ast::SimpleWord::Tilde => bail!("Not implemented yet"),
                                                                        crate::conch::ast::SimpleWord::Colon => bail!("Not implemented yet"),
                                                                    }
                                                                },
                                                                crate::conch::ast::Word::DoubleQuoted(quoted) => {
                                                                    if quoted.len() == 4 {
                                                                        if let crate::conch::ast::SimpleWord::Param(param) = &quoted[0] {
                                                                            if let crate::conch::ast::Parameter::Var(var) = param {
                                                                                if var != "EXPORTS_TOR_DATA_DIR" {
                                                                                    bail!("Not implemented yet");
                                                                                }
                                                                            } else {
                                                                                bail!("Not implemented yet");
                                                                            }
                                                                        } else {
                                                                            bail!("Not implemented yet");
                                                                        }
                                                                        if let crate::conch::ast::SimpleWord::Literal(literal) = &quoted[1] {
                                                                            if literal != "/app-" {
                                                                                bail!("Not implemented yet");
                                                                            }
                                                                        } else {
                                                                            bail!("Not implemented yet");
                                                                        }
                                                                        if let crate::conch::ast::SimpleWord::Param(param) = &quoted[2] {
                                                                            if let crate::conch::ast::Parameter::Var(var) = param {
                                                                                if var != "EXPORTS_APP_ID" {
                                                                                    bail!("Not implemented yet");
                                                                                }
                                                                            } else {
                                                                                bail!("Not implemented yet");
                                                                            }
                                                                        } else {
                                                                            bail!("Not implemented yet");
                                                                        }
                                                                        if let crate::conch::ast::SimpleWord::Literal(literal) = &quoted[3] {
                                                                            if literal.starts_with('-') && literal.ends_with("/hostname") {
                                                                                local_env_vars.insert(name, LocalEnvVar::TorDataDir(literal[1..literal.len() - 9].to_string()));
                                                                            }
                                                                        } else {
                                                                            bail!("Not implemented yet");
                                                                        }
                                                                    } else {
                                                                        bail!("Not implemented yet");
                                                                    }
                                                                },
                                                                crate::conch::ast::Word::SingleQuoted(_) => bail!("Not implemented yet"),
                                                            }
                                                        },
                                                    }
                                            }
                                        }
                                    }
                                }
                                for word in cmd.redirects_or_cmd_words {
                                    match word {
                                        crate::conch::ast::RedirectOrCmdWord::Redirect(_) => {
                                            bail!("Not implemented yet")
                                        }
                                        crate::conch::ast::RedirectOrCmdWord::CmdWord(word) => {
                                            match word.0 {
                                                    crate::conch::ast::ComplexWord::Concat(concat) => {
                                                        for val in concat {
                                                            match val {
                                                                crate::conch::ast::Word::Simple(simple) => {
                                                                    match simple {
                                                                        crate::conch::ast::SimpleWord::Literal(lit) => {
                                                                            if is_env_var_decl && declares_env_var.is_none() && ENV_VAR_REGEX.is_match(&lit) {
                                                                                let without_suffix = &lit[0..lit.len() - 1];
                                                                                declares_env_var = Some(without_suffix.to_string());
                                                                            } else if is_env_var_decl && declares_env_var.is_some() && env_var_value.is_none() {
                                                                                env_var_value = Some(lit);
                                                                            } else {
                                                                                println!("Unexpected literal: {lit}");
                                                                                bail!("Not implemented yet");
                                                                            }
                                                                        },
                                                                        crate::conch::ast::SimpleWord::Escaped(_) => bail!("Not implemented yet"),
                                                                        crate::conch::ast::SimpleWord::Param(_) => bail!("Not implemented yet"),
                                                                        crate::conch::ast::SimpleWord::Subst(_) => bail!("Not implemented yet"),
                                                                        crate::conch::ast::SimpleWord::Star => bail!("Not implemented yet"),
                                                                        crate::conch::ast::SimpleWord::Question => bail!("Not implemented yet"),
                                                                        crate::conch::ast::SimpleWord::SquareOpen => bail!("Not implemented yet"),
                                                                        crate::conch::ast::SimpleWord::SquareClose => bail!("Not implemented yet"),
                                                                        crate::conch::ast::SimpleWord::Tilde => bail!("Not implemented yet"),
                                                                        crate::conch::ast::SimpleWord::Colon => bail!("Not implemented yet"),
                                                                    }
                                                                },
                                                                crate::conch::ast::Word::DoubleQuoted(quoted) => {
                                                                    if quoted.len() != 1 {
                                                                        let mut real_value = String::new();
                                                                        for value in quoted {
                                                                            match value {
                                                                                crate::conch::ast::SimpleWord::Literal(lit) => {
                                                                                    real_value += &lit;
                                                                                },
                                                                                crate::conch::ast::SimpleWord::Escaped(_) => bail!("Not implemented yet"),
                                                                                crate::conch::ast::SimpleWord::Param(param) => {
                                                                                    match param {
                                                                                        crate::conch::ast::Parameter::At => bail!("Not implemented yet"),
                                                                                        crate::conch::ast::Parameter::Star => bail!("Not implemented yet"),
                                                                                        crate::conch::ast::Parameter::Pound => bail!("Not implemented yet"),
                                                                                        crate::conch::ast::Parameter::Question => bail!("Not implemented yet"),
                                                                                        crate::conch::ast::Parameter::Dash => bail!("Not implemented yet"),
                                                                                        crate::conch::ast::Parameter::Dollar => bail!("Not implemented yet"),
                                                                                        crate::conch::ast::Parameter::Bang => bail!("Not implemented yet"),
                                                                                        crate::conch::ast::Parameter::Positional(_) => bail!("Not implemented yet"),
                                                                                        crate::conch::ast::Parameter::Var(var) => {
                                                                                            if let Some(value) = local_env_vars.get(&var) {
                                                                                                match value {
                                                                                                    LocalEnvVar::Literal(literal) => {
                                                                                                        real_value += literal;
                                                                                                    },
                                                                                                    LocalEnvVar::TorDataDir(_) => bail!("Not implemented yet"),
                                                                                                }
                                                                                            } else if let Some(value) = env_vars.get(&var) {
                                                                                                real_value += value;
                                                                                            } else if var.ends_with("_IP") {
                                                                                                let mut key = var.clone();
                                                                                                // This should have the format APP_{APP_NAME}_{SERVICE_NAME}_IP
                                                                                                // Extract the service name
                                                                                                // App name is metadata.id.to_uppercase()
                                                                                                let service_name = var.trim_start_matches(format!("APP_{}_", metadata.id.to_uppercase().replace('-', "_")).as_str()).trim_end_matches("_IP").to_lowercase().replace('_', "-");
                                                                                                // This difference in names is used by some umbrel apps, including Suredbits
                                                                                                if !services.contains_key(&service_name) && services.contains_key(&service_name.replace('-', "")) {
                                                                                                    key = key.replace('_', "");
                                                                                                }

                                                                                                real_value += format!("${{{key}}}").as_str();
                                                                                            } else {
                                                                                                println!("Unknown variable: {var}");
                                                                                                println!("{env_vars:#?}");
                                                                                                bail!("Not implemented yet");
                                                                                            }
                                                                                        },
                                                                                    }
                                                                                },
                                                                                crate::conch::ast::SimpleWord::Subst(_) => bail!("Not implemented yet"),
                                                                                crate::conch::ast::SimpleWord::Star => bail!("Not implemented yet"),
                                                                                crate::conch::ast::SimpleWord::Question => bail!("Not implemented yet"),
                                                                                crate::conch::ast::SimpleWord::SquareOpen => bail!("Not implemented yet"),
                                                                                crate::conch::ast::SimpleWord::SquareClose => bail!("Not implemented yet"),
                                                                                crate::conch::ast::SimpleWord::Tilde => bail!("Not implemented yet"),
                                                                                crate::conch::ast::SimpleWord::Colon => bail!("Not implemented yet"),
                                                                            }
                                                                        }
                                                                        if is_env_var_decl && declares_env_var.is_some() && env_var_value.is_none() {
                                                                            env_var_value = Some(real_value);
                                                                        } else {
                                                                            println!("Unexpected value: {real_value}");
                                                                            bail!("Not implemented yet");
                                                                        }
                                                                    } else if declares_env_var.is_some() && env_var_value.is_none() {
                                                                        match &quoted[0] {
                                                                            crate::conch::ast::SimpleWord::Literal(lit) => {
                                                                                if is_env_var_decl && declares_env_var.is_some() && env_var_value.is_none() {
                                                                                    env_var_value = Some(lit.to_owned());
                                                                                } else {
                                                                                    println!("Unexpected literal: {lit}");
                                                                                    bail!("Not implemented yet");
                                                                                }
                                                                            },
                                                                            crate::conch::ast::SimpleWord::Escaped(_) => bail!("Not implemented yet"),
                                                                            crate::conch::ast::SimpleWord::Param(_) => bail!("Not implemented yet"),
                                                                            crate::conch::ast::SimpleWord::Subst(subst) => {
                                                                                match subst.as_ref() {
                                                                                    crate::conch::ast::ParameterSubstitution::Command(cmd) => {
                                                                                        if cmd.len() == 1 {
                                                                                            match &cmd[0].0 {
                                                                                                crate::conch::ast::Command::Job(_) => bail!("Not implemented yet"),
                                                                                                crate::conch::ast::Command::List(list) => {
                                                                                                    match &list.first {
                                                                                                        crate::conch::ast::ListableCommand::Pipe(_, _) => bail!("Not implemented yet"),
                                                                                                        crate::conch::ast::ListableCommand::Single(single) => {
                                                                                                            match single {
                                                                                                                crate::conch::ast::PipeableCommand::Simple(simple) => {
                                                                                                                    if !simple.redirects_or_env_vars.is_empty() {
                                                                                                                        bail!("Not implemented yet");
                                                                                                                    }
                                                                                                                    if !simple.redirects_or_cmd_words.is_empty() {
                                                                                                                        for (i, thing) in simple.redirects_or_cmd_words.clone().into_iter().enumerate() {
                                                                                                                            match thing {
                                                                                                                                crate::conch::ast::RedirectOrCmdWord::Redirect(redirect) => {
                                                                                                                                    match redirect {
                                                                                                                                        crate::conch::ast::Redirect::Read(_, _) => bail!("Not implemented yet"),
                                                                                                                                        crate::conch::ast::Redirect::Write(what, target) => {
                                                                                                                                            if what == Some(2) && target == crate::conch::ast::ComplexWord::Single(crate::conch::ast::Word::Simple(crate::conch::ast::SimpleWord::Literal("/dev/null".to_string()))) {
                                                                                                                                                // This is to ignore errors, we can ignore it
                                                                                                                                            }
                                                                                                                                        },
                                                                                                                                        crate::conch::ast::Redirect::ReadWrite(_, _) => bail!("Not implemented yet"),
                                                                                                                                        crate::conch::ast::Redirect::Append(_, _) => bail!("Not implemented yet"),
                                                                                                                                        crate::conch::ast::Redirect::Clobber(_, _) => bail!("Not implemented yet"),
                                                                                                                                        crate::conch::ast::Redirect::Heredoc(_, _) => bail!("Not implemented yet"),
                                                                                                                                        crate::conch::ast::Redirect::DupRead(_, _) => bail!("Not implemented yet"),
                                                                                                                                        crate::conch::ast::Redirect::DupWrite(_, _) => bail!("Not implemented yet"),
                                                                                                                                    }
                                                                                                                                },
                                                                                                                                crate::conch::ast::RedirectOrCmdWord::CmdWord(word) =>{
                                                                                                                                    match &word.0 {
                                                                                                                                        crate::conch::ast::ComplexWord::Concat(_) => bail!("Not implemented yet"),
                                                                                                                                        crate::conch::ast::ComplexWord::Single(single) => {
                                                                                                                                            match single {
                                                                                                                                                crate::conch::ast::Word::Simple(simple) => {
                                                                                                                                                    match simple {
                                                                                                                                                        #[allow(clippy::if_same_then_else)]
                                                                                                                                                        crate::conch::ast::SimpleWord::Literal(lit) => {
                                                                                                                                                            if i == 0 && lit != "cat" {
                                                                                                                                                                bail!("Not implemented yet");
                                                                                                                                                            } else if i != 0 {
                                                                                                                                                                bail!("Not implemented yet");
                                                                                                                                                            }
                                                                                                                                                        },
                                                                                                                                                        crate::conch::ast::SimpleWord::Escaped(_) => bail!("Not implemented yet"),
                                                                                                                                                        crate::conch::ast::SimpleWord::Param(_) => bail!("Not implemented yet"),
                                                                                                                                                        crate::conch::ast::SimpleWord::Subst(_) => bail!("Not implemented yet"),
                                                                                                                                                        crate::conch::ast::SimpleWord::Star => bail!("Not implemented yet"),
                                                                                                                                                        crate::conch::ast::SimpleWord::Question => bail!("Not implemented yet"),
                                                                                                                                                        crate::conch::ast::SimpleWord::SquareOpen => bail!("Not implemented yet"),
                                                                                                                                                        crate::conch::ast::SimpleWord::SquareClose => bail!("Not implemented yet"),
                                                                                                                                                        crate::conch::ast::SimpleWord::Tilde => bail!("Not implemented yet"),
                                                                                                                                                        crate::conch::ast::SimpleWord::Colon => bail!("Not implemented yet"),
                                                                                                                                                    }
                                                                                                                                                },
                                                                                                                                                crate::conch::ast::Word::DoubleQuoted(quoted) => {
                                                                                                                                                    if quoted.len() != 1 {
                                                                                                                                                        bail!("Not implemented yet");
                                                                                                                                                    }
                                                                                                                                                    match &quoted[0] {
                                                                                                                                                        crate::conch::ast::SimpleWord::Literal(_) => bail!("Not implemented yet"),
                                                                                                                                                        crate::conch::ast::SimpleWord::Escaped(_) => bail!("Not implemented yet"),
                                                                                                                                                        crate::conch::ast::SimpleWord::Param(param) => {
                                                                                                                                                            match param {
                                                                                                                                                                crate::conch::ast::Parameter::At => bail!("Not implemented yet"),
                                                                                                                                                                crate::conch::ast::Parameter::Star => bail!("Not implemented yet"),
                                                                                                                                                                crate::conch::ast::Parameter::Pound => bail!("Not implemented yet"),
                                                                                                                                                                crate::conch::ast::Parameter::Question => bail!("Not implemented yet"),
                                                                                                                                                                crate::conch::ast::Parameter::Dash => bail!("Not implemented yet"),
                                                                                                                                                                crate::conch::ast::Parameter::Dollar => bail!("Not implemented yet"),
                                                                                                                                                                crate::conch::ast::Parameter::Bang => bail!("Not implemented yet"),
                                                                                                                                                                crate::conch::ast::Parameter::Positional(_) => bail!("Not implemented yet"),
                                                                                                                                                                crate::conch::ast::Parameter::Var(var_name) => {
                                                                                                                                                                    if let Some(thing) = local_env_vars.get(var_name) {
                                                                                                                                                                        match thing {
                                                                                                                                                                            LocalEnvVar::Literal(_) => bail!("Not implemented yet"),
                                                                                                                                                                            LocalEnvVar::TorDataDir(tor_data_dir) => {
                                                                                                                                                                                env_var_value = Some(format!("$APP_HIDDEN_SERVICE_{}", tor_data_dir.to_uppercase().replace('-', "_")));
                                                                                                                                                                            },
                                                                                                                                                                        }
                                                                                                                                                                    } else {
                                                                                                                                                                        println!("Unknown env var: {var_name}");
                                                                                                                                                                        bail!("Not implemented yet");
                                                                                                                                                                    }
                                                                                                                                                                },
                                                                                                                                                            }
                                                                                                                                                        },
                                                                                                                                                        crate::conch::ast::SimpleWord::Subst(_) => bail!("Not implemented yet"),
                                                                                                                                                        crate::conch::ast::SimpleWord::Star => bail!("Not implemented yet"),
                                                                                                                                                        crate::conch::ast::SimpleWord::Question => bail!("Not implemented yet"),
                                                                                                                                                        crate::conch::ast::SimpleWord::SquareOpen => bail!("Not implemented yet"),
                                                                                                                                                        crate::conch::ast::SimpleWord::SquareClose => bail!("Not implemented yet"),
                                                                                                                                                        crate::conch::ast::SimpleWord::Tilde => bail!("Not implemented yet"),
                                                                                                                                                        crate::conch::ast::SimpleWord::Colon => bail!("Not implemented yet"),
                                                                                                                                                    }
                                                                                                                                                },
                                                                                                                                                crate::conch::ast::Word::SingleQuoted(_) => bail!("Not implemented yet"),
                                                                                                                                            }
                                                                                                                                        },
                                                                                                                                    }
                                                                                                                                },
                                                                                                                            }
                                                                                                                        }
                                                                                                                    }
                                                                                                                },
                                                                                                                crate::conch::ast::PipeableCommand::Compound(_) => bail!("Not implemented yet"),
                                                                                                                crate::conch::ast::PipeableCommand::FunctionDef(_, _) => bail!("Not implemented yet"),
                                                                                                            }
                                                                                                        },
                                                                                                    }
                                                                                                    if !list.rest.is_empty() {
                                                                                                        for thing in &list.rest {
                                                                                                            match thing {
                                                                                                                crate::conch::ast::AndOr::And(_) => bail!("Not implemented yet"),
                                                                                                                crate::conch::ast::AndOr::Or(_or) => {
                                                                                                                    //bail!("Not implemented yet")
                                                                                                                    // Ignore fallbacks for now
                                                                                                                },
                                                                                                            }
                                                                                                        }
                                                                                                    }
                                                                                                },
                                                                                            }
                                                                                        } else {
                                                                                            println!("Unexpected command substitution: {cmd:#?}");
                                                                                            bail!("Not implemented yet");
                                                                                        }
                                                                                    },
                                                                                    crate::conch::ast::ParameterSubstitution::Len(_) => bail!("Not implemented yet"),
                                                                                    crate::conch::ast::ParameterSubstitution::Arith(_) => bail!("Not implemented yet"),
                                                                                    crate::conch::ast::ParameterSubstitution::Default(_, _, _) => bail!("Not implemented yet"),
                                                                                    crate::conch::ast::ParameterSubstitution::Assign(_, _, _) => bail!("Not implemented yet"),
                                                                                    crate::conch::ast::ParameterSubstitution::Error(_, _, _) => bail!("Not implemented yet"),
                                                                                    crate::conch::ast::ParameterSubstitution::Alternative(_, _, _) => bail!("Not implemented yet"),
                                                                                    crate::conch::ast::ParameterSubstitution::RemoveSmallestSuffix(_, _) => bail!("Not implemented yet"),
                                                                                    crate::conch::ast::ParameterSubstitution::RemoveLargestSuffix(_, _) => bail!("Not implemented yet"),
                                                                                    crate::conch::ast::ParameterSubstitution::RemoveSmallestPrefix(_, _) => bail!("Not implemented yet"),
                                                                                    crate::conch::ast::ParameterSubstitution::RemoveLargestPrefix(_, _) => bail!("Not implemented yet"),
                                                                                }
                                                                            },
                                                                            crate::conch::ast::SimpleWord::Star => bail!("Not implemented yet"),
                                                                            crate::conch::ast::SimpleWord::Question => bail!("Not implemented yet"),
                                                                            crate::conch::ast::SimpleWord::SquareOpen => bail!("Not implemented yet"),
                                                                            crate::conch::ast::SimpleWord::SquareClose => bail!("Not implemented yet"),
                                                                            crate::conch::ast::SimpleWord::Tilde => bail!("Not implemented yet"),
                                                                            crate::conch::ast::SimpleWord::Colon => bail!("Not implemented yet"),
                                                                        }
                                                                    } else {
                                                                        println!("Unexpected double quoted word: {quoted:?}");
                                                                        bail!("Not implemented yet");
                                                                    }
                                                                },
                                                                crate::conch::ast::Word::SingleQuoted(_) => bail!("Not implemented yet"),
                                                            }
                                                        }
                                                    },
                                                    crate::conch::ast::ComplexWord::Single(word) => {
                                                        match word {
                                                            crate::conch::ast::Word::Simple(simple) => {
                                                                match simple {
                                                                    crate::conch::ast::SimpleWord::Literal(lit) => {
                                                                        if lit == "export" {
                                                                            is_env_var_decl = true;
                                                                            continue;
                                                                        } else {
                                                                            bail!("Not implemented yet");
                                                                        }
                                                                    },
                                                                    crate::conch::ast::SimpleWord::Escaped(_) => bail!("Not implemented yet"),
                                                                    crate::conch::ast::SimpleWord::Param(_) => bail!("Not implemented yet"),
                                                                    crate::conch::ast::SimpleWord::Subst(_) => bail!("Not implemented yet"),
                                                                    crate::conch::ast::SimpleWord::Star => bail!("Not implemented yet"),
                                                                    crate::conch::ast::SimpleWord::Question => bail!("Not implemented yet"),
                                                                    crate::conch::ast::SimpleWord::SquareOpen => bail!("Not implemented yet"),
                                                                    crate::conch::ast::SimpleWord::SquareClose => bail!("Not implemented yet"),
                                                                    crate::conch::ast::SimpleWord::Tilde => bail!("Not implemented yet"),
                                                                    crate::conch::ast::SimpleWord::Colon => bail!("Not implemented yet"),
                                                                }
                                                            },
                                                            crate::conch::ast::Word::DoubleQuoted(_) => bail!("Not implemented yet"),
                                                            crate::conch::ast::Word::SingleQuoted(_) => bail!("Not implemented yet"),
                                                        }
                                                    },
                                                }
                                        }
                                    }
                                }
                            }
                            crate::conch::ast::PipeableCommand::Compound(compound) => {
                                println!("Unexpected compound command: {compound:#?}");
                                bail!("Not implemented yet");
                            }
                            crate::conch::ast::PipeableCommand::FunctionDef(_, _) => {
                                bail!("Not implemented yet")
                            }
                        },
                    }
                }
            }
            if declares_env_var.is_some() && env_var_value.is_some() {
                let env_var_name = declares_env_var.unwrap();
                let env_var_value = env_var_value.unwrap();
                if !env_var_name.ends_with("_IP") {
                    env_vars.insert(env_var_name, env_var_value);
                } else {
                    let mut key = env_var_name.clone();
                    // This should have the format APP_{APP_NAME}_{SERVICE_NAME}_IP
                    // Extract the service name
                    // App name is metadata.id.to_uppercase()
                    let service_name = env_var_name
                        .trim_start_matches(
                            format!("APP_{}_", metadata.id.to_uppercase().replace('-', "_"))
                                .as_str(),
                        )
                        .trim_end_matches("_IP")
                        .to_lowercase()
                        .replace('_', "-");
                    let uppercase_id = metadata.id.clone().to_uppercase();
                    // This difference in names is not used in practice I think, but I only realized that after implementing this
                    if !services.contains_key(&service_name)
                        && services.contains_key(&service_name.replace('-', ""))
                    {
                        key = format!(
                            "APP_{}_{}_IP",
                            uppercase_id.replace('-', "_"),
                            service_name.replace('-', "")
                        );
                    }
                    // A way that is actually used is leaving everything after the first - of the app name out of the app name and env var
                    let app_name_short = uppercase_id.split('-').next().unwrap();
                    let alt_service_name = env_var_name
                        .trim_start_matches(format!("APP_{app_name_short}_").as_str())
                        .trim_end_matches("_IP")
                        .to_lowercase()
                        .replace('_', "-");
                    if !services.contains_key(&service_name)
                        && services.contains_key(&alt_service_name)
                    {
                        key = format!(
                            "APP_{}_{}_IP",
                            app_name_short,
                            alt_service_name.to_uppercase().replace('-', "_")
                        );
                    } else if !services.contains_key(&service_name)
                        && services.contains_key(&alt_service_name.replace('-', ""))
                    {
                        key = format!(
                            "APP_{}_{}_IP",
                            app_name_short,
                            alt_service_name.to_uppercase().replace('-', "")
                        );
                    }
                    env_vars.insert(env_var_name, format!("${{{key}}}"));
                }
            }
        }
    }

    println!("env_vars: {env_vars:#?}");
    let citadel_app_yml = convert_compose(compose_yml, metadata, &env_vars)?;
    let writer = std::fs::File::create(dir.join("app.yml"))?;
    serde_yaml::to_writer(writer, &citadel_app_yml)?;
    Ok(())
}
