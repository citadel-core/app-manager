# For every folder in umbrel-apps, run
# cargo run -- umbrel-to-citadel umbrel-apps/<folder>
# The test succeeeded if an app.yml is created in umbrel-apps/<folder>
# And the exit code is 0
# If it fails, print the error and continue
import os
import subprocess
import sys

ignoredApps = [
    # Custom implementation on Citadel
    "electrs",
    # Built-in on Citadel
    "bitcoin",
    "lightning",
    "core-lightning",
    # This app is very hacky on Umbrel, and it's available natively on Citadel anyway
    "tailscale",
]

passed = 0
failed = 0
skipped = len(ignoredApps)

for folder in os.listdir("umbrel-apps"):
    # If it's not a directory or a .git folder, skip it
    if not os.path.isdir(os.path.join("umbrel-apps", folder)) or folder == ".git" or folder in ignoredApps:
        continue
    # Delete app.yml if it exists
    if os.path.exists(f"umbrel-apps/{folder}/app.yml"):
        os.remove(f"umbrel-apps/{folder}/app.yml")

    try:
        subprocess.run(
            [
                "cargo",
                "run",
                "--all-features",
                "--",
                "umbrel-to-citadel",
                f"umbrel-apps/{folder}",
            ],
            #capture_output=True,
            text=True,
        )
    except subprocess.CalledProcessError as e:
        print(f"\033[31m[FAILED]\033[0m {folder}")
        failed += 1
        print(e.stderr)
        continue
    if not os.path.exists(f"umbrel-apps/{folder}/app.yml"):
        print(f"\033[31m[FAILED]\033[0m {folder}")
        failed += 1
        continue
    print(f"\033[32m[PASSED]\033[0m {folder}")
    passed += 1

total = passed + failed + skipped
print(f"Passed: {passed}/{total} ({round(passed/total*100, 2)}%)")
print(f"Failed: {failed}/{total} ({round(failed/total*100, 2)}%)")
print(f"Skipped: {skipped}/{total} ({round(skipped/total*100, 2)}%)")
