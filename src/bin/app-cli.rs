use citadel_apps::cli;
#[cfg(all(feature = "umbrel", feature = "dev-tools"))]
use citadel_apps::composegenerator::umbrel::types::Metadata as UmbrelMetadata;
use citadel_apps::composegenerator::v4::types::{AppYml};
use citadel_apps::composegenerator::{convert_config, load_config};
#[cfg(feature = "dev-tools")]
use citadel_apps::{
    composegenerator::{
        compose::types::ComposeSpecification,
        types::ResultYml,
        v3::{convert::v3_to_v4, types::SchemaItemContainers},
    },
    updates::update_app,
};
use clap::{Parser, Subcommand};
use std::path::Path;
use std::process::exit;

#[derive(Subcommand, Debug)]
enum SubCommand {
    /// Convert a citadel app.yml to a result.yml file
    Convert {
        /// The citadel root dir
        citadel_root: String,
    },
    /// Download all apps from the sources.list file
    Download {
        /// The citadel root dir
        citadel_root: String,
    },
    /// Get a JSON schema for the app.yml format
    #[cfg(feature = "dev-tools")]
    Schema {
        /// The version of the app.yml format to get the schema for
        /// (defaults to 4)
        #[clap(short, long, default_value = "4")]
        version: String,
    },
    /// Convert an Umbrel app (by app directory path) to a Citadel app.yml file
    /// Manual fixes may be required to make the app.yml work
    #[cfg(feature = "umbrel")]
    UmbrelToCitadel {
        /// The app directory to run this on
        app: String,
        /// The output file to save the result to
        output: String,
    },
    /// Validate a Citadel app.yml file and check if it could be parsed & converted
    #[cfg(feature = "dev-tools")]
    Validate {
        /// The app file to run this on
        app: String,
        /// The app's ID
        #[clap(short, long)]
        app_name: String,
    },
    /// Update the app inside an app.yml to its latest version
    #[cfg(feature = "dev-tools")]
    Update {
        /// The app file or directory to run this on
        app: String,
        /// A GitHub token
        #[clap(short, long)]
        token: Option<String>,
        /// Whether to include pre-releases
        #[clap(short, long)]
        include_prerelease: bool,
    },
    /// Convert an app.yml v3 to an app.yml v4
    /// v3 added implicit mounts of the bitcoin, lnd and CLN data directories, you can remove them from the output if they are not needed
    #[cfg(feature = "dev-tools")]
    V3ToV4 {
        /// The app file to run this on
        app: String,
    },
}

/// Manage apps on Citadel
#[derive(Parser)]
struct Cli {
    /// The subcommand to run
    #[clap(subcommand)]
    command: SubCommand,
}

#[cfg(feature = "dev-tools")]
async fn update_app_yml(path: &Path, include_prerelease: bool) {
    let app_yml = std::fs::File::open(path).expect("Error opening app definition!");
    let mut parsed_app_yml = load_config(app_yml).expect("Failed to parse app.yml");
    let update_result = update_app(&mut parsed_app_yml, include_prerelease).await;
    if update_result.is_err() {
        return;
    }
    match parsed_app_yml {
        citadel_apps::composegenerator::AppYmlFile::V4(app_yml) => {
            let writer = std::fs::File::create(path).expect("Error opening app definition!");
            serde_yaml::to_writer(writer, &app_yml).expect("Error saving app definition!");
        }
        citadel_apps::composegenerator::AppYmlFile::V3(app_yml) => {
            let writer = std::fs::File::create(path).expect("Error opening app definition!");
            serde_yaml::to_writer(writer, &app_yml).expect("Error saving app definition!");
        }
    }
}
#[tokio::main]
async fn main() {
    env_logger::init();
    let args: Cli = Cli::parse();
    match args.command {
        SubCommand::Convert { citadel_root } => {
            cli::convert_dir(&citadel_root);
        },
        SubCommand::Download { citadel_root } => {
            cli::download_apps(&citadel_root);
        },
        #[cfg(feature = "dev-tools")]
        SubCommand::Schema { version } => match version.as_str() {
            "3" => {
                let schema = schemars::schema_for!(SchemaItemContainers);
                println!("{}", serde_yaml::to_string(&schema).unwrap());
            }
            "4" => {
                let schema = schemars::schema_for!(AppYml);
                println!("{}", serde_yaml::to_string(&schema).unwrap());
            }
            #[cfg(feature = "umbrel")]
            "umbrel" => {
                let schema = schemars::schema_for!(UmbrelMetadata);
                println!("{}", serde_yaml::to_string(&schema).unwrap());
            }
            "result" => {
                let schema = schemars::schema_for!(ResultYml);
                println!("{}", serde_yaml::to_string(&schema).unwrap());
            }
            "compose" => {
                let schema = schemars::schema_for!(ComposeSpecification);
                println!("{}", serde_yaml::to_string(&schema).unwrap());
            }
            _ => {
                eprintln!("Unsupported schema version!");
                exit(1);
            }
        },
        #[cfg(feature = "umbrel")]
        SubCommand::UmbrelToCitadel { app, output } => {
            let app_dir = Path::new(&app);
            let compose_yml = std::fs::File::open(app_dir.join("docker-compose.yml"))
                .expect("Error opening docker-compose.yml!");
            let app_yml = std::fs::File::open(app_dir.join("umbrel-app.yml"))
                .expect("Error opening umbrel-app.yml!");
            let app_yml_parsed: citadel_apps::composegenerator::umbrel::types::Metadata =
                serde_yaml::from_reader(app_yml).expect("Error parsing umbrel-app.yml!");
            let compose_yml_parsed: citadel_apps::composegenerator::compose::types::ComposeSpecification
             = serde_yaml::from_reader(compose_yml).expect("Error parsing docker-compose.yml!");
            let result = citadel_apps::composegenerator::umbrel::convert::convert_compose(
                compose_yml_parsed,
                app_yml_parsed,
            );
            let writer = std::fs::File::create(output).expect("Error creating output file");
            serde_yaml::to_writer(writer, &result).expect("Error saving file!");
        }
        #[cfg(feature = "dev-tools")]
        SubCommand::Validate { app, app_name } => {
            let app_yml = std::fs::File::open(app).expect("Error opening app definition!");
            convert_config(&app_name, &app_yml, &None, &None, &None).expect("App is invalid");
            println!("App is valid!");
        }
        #[cfg(feature = "dev-tools")]
        SubCommand::Update {
            app,
            token,
            include_prerelease,
        } => {
            if let Some(gh_token) = token {
                octocrab::initialise(octocrab::OctocrabBuilder::new().personal_token(gh_token))
                    .expect("Failed to initialise octocrab");
            }
            let path = std::path::Path::new(&app);
            if path.is_file() {
                update_app_yml(path, include_prerelease).await;
            } else if path.is_dir() {
                let app_yml_path = path.join("app.yml");
                if app_yml_path.is_file() {
                    update_app_yml(&app_yml_path, include_prerelease).await;
                } else {
                    let subdirs = std::fs::read_dir(path).expect("Failed to read directory");
                    for subdir in subdirs {
                        let subdir = subdir.unwrap_or_else(|_| {
                            panic!("Failed to read subdir/file in {}", path.display())
                        });
                        let file_type = subdir.file_type().unwrap_or_else(|_| {
                            panic!(
                                "Failed to get filetype of {}/{}",
                                path.display(),
                                subdir.file_name().to_string_lossy()
                            )
                        });
                        if file_type.is_file() {
                            continue;
                        } else if file_type.is_symlink() {
                            eprintln!(
                                "Symlinks like {}/{} are not supported yet!",
                                path.display(),
                                subdir.file_name().to_string_lossy()
                            );
                        } else if file_type.is_dir() {
                            let sub_app_yml = subdir.path().join("app.yml");
                            if sub_app_yml.is_file() {
                                update_app_yml(&sub_app_yml, include_prerelease).await;
                            } else {
                                eprintln!(
                                    "{}/{}/app.yml does not exist or is not a file!",
                                    path.display(),
                                    subdir.file_name().to_string_lossy()
                                );
                                continue;
                            }
                        } else {
                            unreachable!();
                        }
                    }
                }
            } else {
                panic!();
            }
        }
        #[cfg(feature = "dev-tools")]
        SubCommand::V3ToV4 { app } => {
            let app_yml = std::fs::File::open(app.clone()).expect("Error opening app definition!");
            let parsed_app_yml = load_config(app_yml).expect("Failed to parse app.yml");
            match parsed_app_yml {
                citadel_apps::composegenerator::AppYmlFile::V4(_) => {
                    panic!("The app already seems to be an app.yml v4");
                }
                citadel_apps::composegenerator::AppYmlFile::V3(app_yml) => {
                    let writer = std::fs::File::create(app).expect("Error opening app definition!");
                    serde_yaml::to_writer(writer, &v3_to_v4(app_yml, &None))
                        .expect("Error saving app definition!");
                }
            }
        }
    }
}
