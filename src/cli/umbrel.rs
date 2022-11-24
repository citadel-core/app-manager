use anyhow::Result;
use std::path::Path;

use crate::composegenerator::umbrel::convert::convert_compose;
use crate::composegenerator::umbrel::types::Metadata;
use crate::composegenerator::compose::types::ComposeSpecification;

/// Takes a directory that contains an Umbrel app and check if it can run on Citadel, if possible, port it to Citadel
/// The app.yml will be written to the same directory
/// The result will indicate success or failure
pub fn convert(dir: &Path) -> Result<()> {
    let umbrel_app_yml = std::fs::File::open(dir.join("umbrel-app.yml"))?;
    let metadata = serde_yaml::from_reader::<_, Metadata>(umbrel_app_yml)?;
    let compose_yml = std::fs::File::open(dir.join("docker-compose.yml"))?;
    let compose_yml = serde_yaml::from_reader::<_, ComposeSpecification>(compose_yml)?;

    let citadel_app_yml = convert_compose(compose_yml, metadata);
    let writer = std::fs::File::create(dir.join("app.yml"))?;
    serde_yaml::to_writer(writer, &citadel_app_yml)?;
    Ok(())
}
