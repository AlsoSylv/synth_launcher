use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct VersionManifest {
    pub latest: Latest,
    pub versions: Vec<Version>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Latest {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub id: String,
    #[serde(rename = "type")]
    pub version_type: Type,
    pub url: String,
    pub time: String,
    pub release_time: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Type {
    OldAlpha,
    OldBeta,
    Release,
    Snapshot,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VersionJson {
    Modern(Modern),
    Legacy(Legacy),
    Ancient(Ancient),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndex {
    pub id: String,
    pub sha1: String,
    pub size: i64,
    pub total_size: Option<i64>,
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Library {
    pub downloads: LibraryDownloads,
    pub name: String,
    pub rules: Option<Vec<Rule>>,
    pub extract: Option<Extract>,
    pub natives: Option<Natives>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Natives {
    pub linux: Option<String>,
    pub osx: Option<String>,
    pub windows: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Extract {
    pub exclude: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Rule {
    pub action: Action,
    pub os: Option<Os>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Allow,
    Disallow,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Os {
    pub name: String,
    pub version: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LibraryDownloads {
    pub artifact: Option<Artifact>,
    pub classifiers: Option<Classifiers>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Classifiers {
    #[serde(rename = "linux-x86_64")]
    pub linux_x86_64: Option<Artifact>,
    pub natives_linux: Option<Artifact>,
    pub natives_osx: Option<Artifact>,
    pub natives_windows: Option<Artifact>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Artifact {
    pub sha1: String,
    pub size: i64,
    pub url: String,
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AssetIndexJson {
    pub objects: std::collections::HashMap<String, Object>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Object {
    pub hash: String,
    pub size: i64,
}

pub use ancient::Ancient;
pub use legacy::Legacy;
pub use modern::Modern;

pub mod modern {
    use super::{AssetIndex, Library};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Modern {
        pub arguments: Arguments,
        pub asset_index: AssetIndex,
        pub assets: String,
        pub compliance_level: i64,
        pub downloads: WelcomeDownloads,
        pub id: String,
        pub java_version: JavaVersion,
        pub libraries: Vec<Library>,
        pub logging: Logging,
        pub main_class: String,
        pub minimum_launcher_version: i64,
        pub release_time: String,
        pub time: String,
        #[serde(rename = "type")]
        pub welcome_type: String,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct Arguments {
        pub game: Vec<GameElement>,
        pub jvm: Vec<JvmElement>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum GameElement {
        GameClass(GameClass),
        String(String),
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct GameClass {
        pub rules: Vec<GameRule>,
        pub value: Value,
    }

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum Value {
        String(String),
        StringArray(Vec<String>),
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct GameRule {
        pub action: Action,
        pub features: Features,
    }

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Action {
        Allow,
        Disallow,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct Features {
        pub is_demo_user: Option<bool>,
        pub has_custom_resolution: Option<bool>,
        pub has_quick_plays_support: Option<bool>,
        pub is_quick_play_singleplayer: Option<bool>,
        pub is_quick_play_multiplayer: Option<bool>,
        pub is_quick_play_realms: Option<bool>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum JvmElement {
        JvmClass(JvmClass),
        String(String),
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct JvmClass {
        pub rules: Vec<JvmRule>,
        pub value: Value,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct JvmRule {
        pub action: Action,
        pub os: PurpleOs,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct PurpleOs {
        pub name: Option<String>,
        pub arch: Option<String>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct WelcomeDownloads {
        pub client: ClientMappingsClass,
        pub client_mappings: Option<ClientMappingsClass>,
        pub server: ClientMappingsClass,
        pub server_mappings: Option<ClientMappingsClass>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct ClientMappingsClass {
        pub sha1: String,
        pub size: i64,
        pub url: String,
        pub path: Option<String>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct JavaVersion {
        pub component: String,
        pub major_version: i64,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct Logging {
        pub client: LoggingClient,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct LoggingClient {
        pub argument: String,
        pub file: AssetIndex,
        #[serde(rename = "type")]
        pub client_type: String,
    }
}

pub mod legacy {
    use super::{AssetIndex, Library};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Legacy {
        pub asset_index: AssetIndex,
        pub assets: String,
        pub compliance_level: Option<i64>,
        pub downloads: WelcomeDownloads,
        pub id: String,
        pub java_version: Option<JavaVersion>,
        pub libraries: Vec<Library>,
        pub logging: Logging,
        pub main_class: String,
        pub minecraft_arguments: String,
        pub minimum_launcher_version: i64,
        pub release_time: String,
        pub time: String,
        #[serde(rename = "type")]
        pub welcome_type: String,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct WelcomeDownloads {
        pub client: ServerClass,
        pub server: ServerClass,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct ServerClass {
        pub sha1: String,
        pub size: i64,
        pub url: String,
        pub path: Option<String>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct JavaVersion {
        pub component: String,
        pub major_version: i64,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct Logging {
        pub client: LoggingClient,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct LoggingClient {
        pub argument: String,
        pub file: AssetIndex,
        #[serde(rename = "type")]
        pub client_type: String,
    }
}

pub mod ancient {
    use super::{AssetIndex, Library};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Ancient {
        pub asset_index: AssetIndex,
        pub assets: String,
        pub downloads: WelcomeDownloads,
        pub id: String,
        pub libraries: Vec<Library>,
        pub main_class: String,
        pub minecraft_arguments: String,
        pub minimum_launcher_version: i64,
        pub release_time: String,
        pub time: String,
        #[serde(rename = "type")]
        pub welcome_type: String,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct WelcomeDownloads {
        pub client: Client,
        pub server: Option<Client>,
        pub windows_server: Option<Client>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct Client {
        pub sha1: String,
        pub size: i64,
        pub url: String,
        pub path: Option<String>,
    }
}
