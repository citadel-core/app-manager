#[cfg(feature = "schema")]
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

use crate::composegenerator::compose::types::{Command, StringOrIntOrBool};
use crate::composegenerator::types::Permissions;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(untagged)]
pub enum HiddenServices {
    PortMap(HashMap<u16, u16>),
    LayeredMap(HashMap<String, HashMap<u16, u16>>),
}

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct PortsDefinition {
    pub tcp: Option<HashMap<u16, u16>>,
    pub udp: Option<HashMap<u16, u16>>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub enum PortPriority {
    /// Outside port doesn't matter
    Optional,
    /// Outside port is preferred, but not required for the app to work
    Recommended,
    /// Port is required for the app to work
    Required,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(untagged)]
pub enum StringOrMap {
    String(String),
    Map(HashMap<String, String>),
}

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct Container {
    // These can be copied directly without validation
    pub image: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_grace_period: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_signal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depends_on: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub init: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_hosts: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    // These need security checks
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entrypoint: Option<Command>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<Command>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<HashMap<String, StringOrIntOrBool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cap_add: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_mode: Option<String>,
    // These are not directly present in a compose file and need to be converted
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    // This is currently handled on the host
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port_priority: Option<PortPriority>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_ports: Option<PortsDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mounts: Option<HashMap<String, StringOrMap>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assign_fixed_ip: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden_services: Option<HiddenServices>,
}

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct InputMetadata {
    /// The name of the app
    pub name: String,
    /// The version of the app
    pub version: String,
    /// The category for the app
    pub category: String,
    /// A short tagline for the app
    pub tagline: String,
    // Developer name -> their website
    pub developers: HashMap<String, String>,
    /// A description of the app
    pub description: String,
    #[serde(default)]
    /// Permissions the app requires
    pub permissions: Vec<Permissions>,
    /// App repository name -> repo URL
    pub repo: BTreeMap<String, String>,
    /// A support link for the app
    pub support: String,
    /// A list of promo images for the apps
    pub gallery: Option<Vec<String>>,
    /// The path the "Open" link on the dashboard should lead to
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The app's default username
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_username: Option<String>,
    /// The app's default password. Can also be $APP_SEED for a random password
    pub default_password: Option<String>,
    #[serde(default = "bool::default")]
    /// True if the app only works over Tor
    pub tor_only: bool,
    /// A list of containers to update automatically (still validated by the Citadel team)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_containers: Option<Vec<String>>,
    /// For "virtual" apps, the service the app implements
    #[serde(skip_serializing_if = "Option::is_none")]
    pub implements: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_control: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_notes: Option<BTreeMap<String, String>>,
}

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
/// Citadel app definition
pub struct AppYml {
    pub citadel_version: u8,
    pub metadata: InputMetadata,
    pub services: HashMap<String, Container>,
}

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PortMapElement {
    /// True if the port is defined by an env var and can be anything
    pub dynamic: bool,
    pub internal_port: u16,
    pub public_port: u16,
}
