use anyhow::{bail, Result};
use cached::proc_macro::cached;
use octocrab::models::repos::Tag;
use semver::Version;

#[cached(result = true)]
pub async fn get_tags(owner: String, repo: String) -> Result<Vec<Tag>> {
    let octocrab = octocrab::instance();
    let tags = octocrab
        .repos(owner, repo)
        .list_tags()
        .send()
        .await?
        .take_items();
    Ok(tags)
}

pub async fn check_updates(
    owner: &String,
    repo: &String,
    current_version: &Version,
    include_pre: bool,
) -> Result<String> {
    let tags = get_tags(owner.to_owned(), repo.to_owned()).await?;
    for tag in tags {
        let tag = tag.name;
        // Remove the v prefix if it exists
        let tag = tag.trim_start_matches('v');
        let version = Version::parse(tag)?;
        if (include_pre || version.pre.is_empty()) && &version > current_version {
            return Ok(tag.to_string());
        }
    }

    bail!("No update found")
}

// Check if a string is a valid GitHub repository path (https://github.com/owner/repo),
// and return the owner and repo if it is.
pub fn get_repo_path(repo_path: &str) -> Option<(String, String)> {
    if !repo_path.starts_with("https://github.com") {
        return None;
    }
    let repo_path = repo_path.replace("https://github.com/", "");
    let repo_path = repo_path.split('/').collect::<Vec<&str>>();
    if repo_path.len() != 2 {
        return None;
    }
    Some((repo_path[0].to_string(), repo_path[1].to_string()))
}

#[cfg(test)]
mod test {
    use super::get_repo_path;

    #[test]
    fn test_get_repo_path() {
        let repo_path = "https://github.com/AaronDewes/repo";
        let repo_path = get_repo_path(repo_path);
        assert_eq!(
            repo_path,
            Some(("AaronDewes".to_string(), "repo".to_string()))
        );
    }

    #[test]
    fn test_get_repo_path_invalid() {
        let repo_path = "https://github.com/AaronDewes/repo/invalid";
        let repo_path = get_repo_path(repo_path);
        assert!(repo_path.is_none());
    }
    #[test]
    fn test_get_repo_path_not_github() {
        let repo_path = "https://gitlab.com/AaronDewes/repo";
        let repo_path = get_repo_path(repo_path);
        assert!(repo_path.is_none());
    }
}
