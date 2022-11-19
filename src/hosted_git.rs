use gitlab::Gitlab;

use super::composegenerator::v4::types::InputMetadata;
use super::github;
use anyhow::{bail, Result};

pub async fn check_updates(
    metadata: &InputMetadata,
    include_pre: bool,
    token: Option<String>,
) -> Result<String> {
    let current_version = metadata.version.clone();
    let current_version = semver::Version::parse(&current_version)?;
    match metadata
        .version_control
        .clone()
        .unwrap_or_else(|| "github".to_string())
        .to_lowercase()
        .as_str()
    {
        "github" => {
            if let Some(gh_token) = token {
                octocrab::initialise(octocrab::OctocrabBuilder::new().personal_token(gh_token))?;
            }
            let Some(repo) = metadata
            .repo
            .values()
            .next() else {
                bail!("App is missing repository")
            };
            let Some((owner, repo)) = github::get_repo_path(
                repo
                    .as_str(),
            ) else {
                bail!("No repo path found");
            };
            super::github::check_updates(&owner, &repo, &current_version, include_pre).await
        }
        "gitlab" => {
            let Some(repo) = metadata
            .repo
            .values()
            .next() else {
                bail!("App is missing repository")
            };
            let Some((gitlab_server, repo)) = super::gitlab::get_repo_path(
                repo
                    .as_str(),
            ) else {
                bail!("No repo path found");
            };
            let client = Gitlab::builder(gitlab_server, token.unwrap_or_default())
                .build_async()
                .await;
            if let Err(client_err) = client {
                bail!(client_err);
            }
            let client = client.unwrap();
            super::gitlab::check_updates(&client, repo, &current_version, include_pre).await
        }
        _ => bail!("Version control system not supported"),
    }
}
