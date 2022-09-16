# Citadel app CLI

This is a new tool to parse, validate, and process Citadel app.yml and app.yml.jinja files.
In addition, it can also parse Umbrel apps and convert them to Citadel apps. Because this feature is somewhat limited and may require some manual work, it is not used automatically.

It is currently invoked by a Python script and is not intended to be used directly by users.

However, it can be quite useful for developers who want to validate their app.yml files or port their app from Umbrel.

## Getting started

---

🛈 To compile this app, you need to have Rust installed. You can find a guide on how to install Rust [here](https://rustup.rs/).

---

### Building a developer version

To build a version for development, run the following command:

```
cargo build --bin app-cli --release --all-features
```

### Building a user version

*If you are planning to actually use this CLI during development, you should rather use a developer version. The user build disables some features to reduce the size of the binary.*

To compile the CLI in a minimal version, like the one we ship to end users, run:

```
cargo build --bin app-cli --release --features=cli,preprocess,umbrel
```


### Subcommands

Run `app-cli help` to see a list of available subcommands and their usage.

