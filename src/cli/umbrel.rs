use anyhow::Result;
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

/// Takes a directory that contains an Umbrel app and check if it can run on Citadel, if possible, port it to Citadel
/// The app.yml will be written to the same directory
/// The result will indicate success or failure
pub fn convert(dir: &Path) -> Result<()> {
    let umbrel_app_yml = std::fs::File::open(dir.join("umbrel-app.yml"))?;
    let metadata = serde_yaml::from_reader::<_, Metadata>(umbrel_app_yml)?;
    let compose_yml = std::fs::File::open(dir.join("docker-compose.yml"))?;
    let compose_yml = serde_yaml::from_reader::<_, ComposeSpecification>(compose_yml)?;

    let mut env_vars = HashMap::<String, String>::new();
    let exports_sh = dir.join("exports.sh");
    if exports_sh.exists() {
        let exports_sh = std::fs::read_to_string(exports_sh)?;
        let mut local_env_vars = HashMap::new();

        let lexer = Lexer::new(exports_sh.chars());
        let parser = DefaultParser::new(lexer);
        for t in parser {
            if let Err(e) = t {
                eprintln!("Error parsing exports.sh: {}", e);
                break;
            }
            let t = t.unwrap();
            let t = t.0;
            let mut is_env_var_decl = false;
            let mut declares_env_var = None;
            let mut env_var_value = None;
            match t {
                crate::conch::ast::Command::Job(_) => todo!(),
                crate::conch::ast::Command::List(list) => {
                    if !list.rest.is_empty() {
                        todo!();
                    }
                    match list.first {
                        crate::conch::ast::ListableCommand::Pipe(_, _) => todo!(),
                        crate::conch::ast::ListableCommand::Single(cmd) => match cmd {
                            crate::conch::ast::PipeableCommand::Simple(cmd) => {
                                if !cmd.redirects_or_env_vars.is_empty() {
                                    for thing in cmd.redirects_or_env_vars {
                                        match thing {
                                            crate::conch::ast::RedirectOrEnvVar::Redirect(_) => {
                                                todo!()
                                            }
                                            crate::conch::ast::RedirectOrEnvVar::EnvVar(
                                                name,
                                                val,
                                            ) => {
                                                if val.is_none() {
                                                    eprintln!("Error parsing exports.sh: env var {} has no value", name);
                                                    continue;
                                                }
                                                match val.unwrap().0 {
                                                        crate::conch::ast::ComplexWord::Concat(_) => todo!(),
                                                        crate::conch::ast::ComplexWord::Single(single) => {
                                                            match single {
                                                                crate::conch::ast::Word::Simple(simple) => {
                                                                    match simple {
                                                                        crate::conch::ast::SimpleWord::Literal(lit) => {
                                                                            local_env_vars.insert(name, LocalEnvVar::Literal(lit));
                                                                        },
                                                                        crate::conch::ast::SimpleWord::Escaped(_) => todo!(),
                                                                        crate::conch::ast::SimpleWord::Param(_) => todo!(),
                                                                        crate::conch::ast::SimpleWord::Subst(_) => todo!(),
                                                                        crate::conch::ast::SimpleWord::Star => todo!(),
                                                                        crate::conch::ast::SimpleWord::Question => todo!(),
                                                                        crate::conch::ast::SimpleWord::SquareOpen => todo!(),
                                                                        crate::conch::ast::SimpleWord::SquareClose => todo!(),
                                                                        crate::conch::ast::SimpleWord::Tilde => todo!(),
                                                                        crate::conch::ast::SimpleWord::Colon => todo!(),
                                                                    }
                                                                },
                                                                crate::conch::ast::Word::DoubleQuoted(quoted) => {
                                                                    if quoted.len() == 4 {
                                                                        if let crate::conch::ast::SimpleWord::Param(param) = &quoted[0] {
                                                                            if let crate::conch::ast::Parameter::Var(var) = param {
                                                                                if var != "EXPORTS_TOR_DATA_DIR" {
                                                                                    todo!();
                                                                                }
                                                                            } else {
                                                                                todo!();
                                                                            }
                                                                        } else {
                                                                            todo!();
                                                                        }
                                                                        if let crate::conch::ast::SimpleWord::Literal(literal) = &quoted[1] {
                                                                            if literal != "/app-" {
                                                                                todo!();
                                                                            }
                                                                        } else {
                                                                            todo!();
                                                                        }
                                                                        if let crate::conch::ast::SimpleWord::Param(param) = &quoted[2] {
                                                                            if let crate::conch::ast::Parameter::Var(var) = param {
                                                                                if var != "EXPORTS_APP_ID" {
                                                                                    todo!();
                                                                                }
                                                                            } else {
                                                                                todo!();
                                                                            }
                                                                        } else {
                                                                            todo!();
                                                                        }
                                                                        if let crate::conch::ast::SimpleWord::Literal(literal) = &quoted[3] {
                                                                            if literal.starts_with('-') && literal.ends_with("/hostname") {
                                                                                local_env_vars.insert(name, LocalEnvVar::TorDataDir(literal[1..literal.len() - 9].to_string()));
                                                                            }
                                                                        } else {
                                                                            todo!();
                                                                        }
                                                                    } else {
                                                                        todo!();
                                                                    }
                                                                },
                                                                crate::conch::ast::Word::SingleQuoted(_) => todo!(),
                                                            }
                                                        },
                                                    }
                                            }
                                        }
                                    }
                                }
                                for word in cmd.redirects_or_cmd_words {
                                    match word {
                                            crate::conch::ast::RedirectOrCmdWord::Redirect(_) => todo!(),
                                            crate::conch::ast::RedirectOrCmdWord::CmdWord(word) => {
                                                match word.0 {
                                                    crate::conch::ast::ComplexWord::Concat(concat) => {
                                                        for val in concat {
                                                            match val {
                                                                crate::conch::ast::Word::Simple(simple) => {
                                                                    match simple {
                                                                        crate::conch::ast::SimpleWord::Literal(lit) => {
                                                                            if is_env_var_decl && declares_env_var.is_none() && ENV_VAR_REGEX.is_match(&lit) {
                                                                                declares_env_var = Some(lit);
                                                                            } else if is_env_var_decl && declares_env_var.is_some() && env_var_value.is_none() {
                                                                                env_var_value = Some(lit);
                                                                            } else {
                                                                                println!("Unexpected literal: {}", lit);
                                                                                todo!();
                                                                            }
                                                                        },
                                                                        crate::conch::ast::SimpleWord::Escaped(_) => todo!(),
                                                                        crate::conch::ast::SimpleWord::Param(_) => todo!(),
                                                                        crate::conch::ast::SimpleWord::Subst(_) => todo!(),
                                                                        crate::conch::ast::SimpleWord::Star => todo!(),
                                                                        crate::conch::ast::SimpleWord::Question => todo!(),
                                                                        crate::conch::ast::SimpleWord::SquareOpen => todo!(),
                                                                        crate::conch::ast::SimpleWord::SquareClose => todo!(),
                                                                        crate::conch::ast::SimpleWord::Tilde => todo!(),
                                                                        crate::conch::ast::SimpleWord::Colon => todo!(),
                                                                    }
                                                                },
                                                                crate::conch::ast::Word::DoubleQuoted(quoted) => {
                                                                    if quoted.len() != 1 {
                                                                        println!("Unexpected double quoted: {:#?}", quoted);
                                                                        todo!();
                                                                    } else if declares_env_var.is_some() && env_var_value.is_none() {
                                                                        match &quoted[0] {
                                                                            crate::conch::ast::SimpleWord::Literal(lit) => {
                                                                                if is_env_var_decl && declares_env_var.is_some() && env_var_value.is_none() {
                                                                                    env_var_value = Some(lit.to_owned());
                                                                                } else {
                                                                                    println!("Unexpected literal: {}", lit);
                                                                                    todo!();
                                                                                }
                                                                            },
                                                                            crate::conch::ast::SimpleWord::Escaped(_) => todo!(),
                                                                            crate::conch::ast::SimpleWord::Param(_) => todo!(),
                                                                            crate::conch::ast::SimpleWord::Subst(subst) => {
                                                                                match subst.as_ref() {
                                                                                    crate::conch::ast::ParameterSubstitution::Command(cmd) => {
                                                                                        if cmd.len() == 1 {
                                                                                            match &cmd[0].0 {
                                                                                                crate::conch::ast::Command::Job(_) => todo!(),
                                                                                                crate::conch::ast::Command::List(list) => {
                                                                                                    println!("Unexpected command substitution list: {:#?}", list);
                                                                                                    todo!();
                                                                                                },
                                                                                            }
                                                                                        } else {
                                                                                            println!("Unexpected command substitution: {:#?}", cmd);
                                                                                            todo!();
                                                                                        }
                                                                                    },
                                                                                    crate::conch::ast::ParameterSubstitution::Len(_) => todo!(),
                                                                                    crate::conch::ast::ParameterSubstitution::Arith(_) => todo!(),
                                                                                    crate::conch::ast::ParameterSubstitution::Default(_, _, _) => todo!(),
                                                                                    crate::conch::ast::ParameterSubstitution::Assign(_, _, _) => todo!(),
                                                                                    crate::conch::ast::ParameterSubstitution::Error(_, _, _) => todo!(),
                                                                                    crate::conch::ast::ParameterSubstitution::Alternative(_, _, _) => todo!(),
                                                                                    crate::conch::ast::ParameterSubstitution::RemoveSmallestSuffix(_, _) => todo!(),
                                                                                    crate::conch::ast::ParameterSubstitution::RemoveLargestSuffix(_, _) => todo!(),
                                                                                    crate::conch::ast::ParameterSubstitution::RemoveSmallestPrefix(_, _) => todo!(),
                                                                                    crate::conch::ast::ParameterSubstitution::RemoveLargestPrefix(_, _) => todo!(),
                                                                                }
                                                                            },
                                                                            crate::conch::ast::SimpleWord::Star => todo!(),
                                                                            crate::conch::ast::SimpleWord::Question => todo!(),
                                                                            crate::conch::ast::SimpleWord::SquareOpen => todo!(),
                                                                            crate::conch::ast::SimpleWord::SquareClose => todo!(),
                                                                            crate::conch::ast::SimpleWord::Tilde => todo!(),
                                                                            crate::conch::ast::SimpleWord::Colon => todo!(),
                                                                        }
                                                                    } else {
                                                                        println!("Unexpected double quoted word: {:?}", quoted);
                                                                        todo!();
                                                                    }
                                                                },
                                                                crate::conch::ast::Word::SingleQuoted(_) => todo!(),
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
                                                                            todo!();
                                                                        }
                                                                    },
                                                                    crate::conch::ast::SimpleWord::Escaped(_) => todo!(),
                                                                    crate::conch::ast::SimpleWord::Param(_) => todo!(),
                                                                    crate::conch::ast::SimpleWord::Subst(_) => todo!(),
                                                                    crate::conch::ast::SimpleWord::Star => todo!(),
                                                                    crate::conch::ast::SimpleWord::Question => todo!(),
                                                                    crate::conch::ast::SimpleWord::SquareOpen => todo!(),
                                                                    crate::conch::ast::SimpleWord::SquareClose => todo!(),
                                                                    crate::conch::ast::SimpleWord::Tilde => todo!(),
                                                                    crate::conch::ast::SimpleWord::Colon => todo!(),
                                                                }
                                                            },
                                                            crate::conch::ast::Word::DoubleQuoted(_) => todo!(),
                                                            crate::conch::ast::Word::SingleQuoted(_) => todo!(),
                                                        }
                                                    },
                                                }
                                            },
                                        }
                                }
                            }
                            crate::conch::ast::PipeableCommand::Compound(_) => todo!(),
                            crate::conch::ast::PipeableCommand::FunctionDef(_, _) => todo!(),
                        },
                    }
                }
            }
            if declares_env_var.is_some() && env_var_value.is_some() {
                env_vars.insert(declares_env_var.unwrap(), env_var_value.unwrap());
            }
        }
    }

    let citadel_app_yml = convert_compose(compose_yml, metadata);
    let writer = std::fs::File::create(dir.join("app.yml"))?;
    serde_yaml::to_writer(writer, &citadel_app_yml)?;
    Ok(())
}
