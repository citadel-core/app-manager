use tempdir::TempDir;

pub fn download_apps(citadel_root: &str) {
    // Create a temporary directory to download the apps to
    let temp_dir = TempDir::new("citadel_apps").unwrap();
    let temp_dir_path = temp_dir.path();
    let clone_result =
        git_repository::prepare_clone("https://github.com/runcitadel/apps", temp_dir_path);
    if clone_result.is_err() {
        println!("Failed to clone apps repository");
        return;
    }
    let clone_result = clone_result
        .unwrap()
        .fetch_only(
            git_repository::progress::Discard,
            &std::sync::atomic::AtomicBool::default(),
        );
    if clone_result.is_err() {
        println!("Failed to fetch apps repository");
        println!("{:?}", clone_result.err().unwrap());
        return;
    }
    println!("Downloaded apps to {}", temp_dir_path.display());
}
