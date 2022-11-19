use std::cell::RefCell;
use std::io::{self, Write};
use std::path::{Path, PathBuf};


use git2::build::{CheckoutBuilder, RepoBuilder};
use git2::{FetchOptions, Progress, RemoteCallbacks, Repository};
use std::{collections::HashMap};
use anyhow::Result;

struct State {
    progress: Option<Progress<'static>>,
    total: usize,
    current: usize,
    path: Option<PathBuf>,
    newline: bool,
}

fn print(state: &mut State) {
    let stats = state.progress.as_ref().unwrap();
    let network_pct = (100 * stats.received_objects()) / stats.total_objects();
    let index_pct = (100 * stats.indexed_objects()) / stats.total_objects();
    let co_pct = if state.total > 0 {
        (100 * state.current) / state.total
    } else {
        0
    };
    let kbytes = stats.received_bytes() / 1024;
    if stats.received_objects() == stats.total_objects() {
        if !state.newline {
            println!();
            state.newline = true;
        }
        print!(
            "Resolving deltas {}/{}\r",
            stats.indexed_deltas(),
            stats.total_deltas()
        );
    } else {
        print!(
            "net {:3}% ({:4} kb, {:5}/{:5})  /  idx {:3}% ({:5}/{:5})  \
             /  chk {:3}% ({:4}/{:4}) {}\r",
            network_pct,
            kbytes,
            stats.received_objects(),
            stats.total_objects(),
            index_pct,
            stats.indexed_objects(),
            stats.total_objects(),
            co_pct,
            state.current,
            state.total,
            state
                .path
                .as_ref()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        )
    }
    io::stdout().flush().unwrap();
}

pub fn clone(repo: &str, branch: &str, target: &Path) -> Result<(), git2::Error> {
    let state = RefCell::new(State {
        progress: None,
        total: 0,
        current: 0,
        path: None,
        newline: false,
    });
    let mut cb = RemoteCallbacks::new();
    cb.transfer_progress(|stats| {
        let mut state = state.borrow_mut();
        state.progress = Some(stats.to_owned());
        print(&mut state);
        true
    });

    let mut co = CheckoutBuilder::new();
    co.progress(|path, cur, total| {
        let mut state = state.borrow_mut();
        state.path = path.map(|p| p.to_path_buf());
        state.current = cur;
        state.total = total;
        print(&mut state);
    });

    let mut fo = FetchOptions::new();
    fo.remote_callbacks(cb);
    RepoBuilder::new()
        .fetch_options(fo)
        .with_checkout(co)
        .branch(branch)
        .clone(repo, target)?;
    println!();

    Ok(())
}

/// Gets the currently checked out commit from a repo path
pub fn get_commit(repo_path: &Path) -> Result<String, git2::Error> {
    let repo = git2::Repository::open(repo_path)?;
    let head = repo.head()?;
    let commit = head.peel_to_commit()?;
    Ok(commit.id().to_string())
}

pub fn get_latest_commit_for_apps(repo_path: &Path, app_subdir: &str, apps: &[String]) -> Result<HashMap<String, String>> {
        let mut latest_commits: HashMap<String, String> = HashMap::new();
        let repo = Repository::open(repo_path)?;
        let mut revwalk = repo.revwalk()?;
        revwalk.set_sorting(git2::Sort::TIME)?;
        revwalk.push_head()?;
        for commit_id in revwalk {
            let commit_id = commit_id?;
            let commit = repo.find_commit(commit_id)?;
            // Ignore merge commits (2+ parents) because that's what 'git whatchanged' does.
            // Ignore commit with 0 parents (initial commit) because there's nothing to diff against
            if commit.parent_count() == 1 {
                let prev_commit = commit.parent(0)?;
                let tree = commit.tree()?;
                let prev_tree = prev_commit.tree()?;
                let diff= repo.diff_tree_to_tree(Some(&prev_tree), Some(&tree), None)?;
                for delta in diff.deltas() {
                    let file_path = delta.new_file().path().unwrap();
                    if file_path.starts_with(app_subdir) {
                        let app_name = file_path.components().nth(1).unwrap().as_os_str().to_str().unwrap();
                        if apps.contains(&app_name.to_string()) && !latest_commits.contains_key(app_name) {
                            latest_commits.insert(app_name.to_string(), commit.id().to_string());
                        }
                    }
                }
            }
        }
        Ok(latest_commits)
}
