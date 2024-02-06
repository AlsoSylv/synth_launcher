use std::ops::Deref;
use serde::{Deserialize, Deserializer, Serialize};
use serde_with::skip_serializing_none;
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize)]
pub struct VersionManifest {
    pub latest: Latest,
    pub versions: Vec<Version>,
}

impl VersionManifest {
    pub fn latest_release(&self) -> &Version {
        for version in &self.versions {
            if version.id == self.latest.release {
                return version;
            }
        }

        // If the latest release does not exist in the meta, things have probably gone wrong lol
        unreachable!()
    }
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct Latest {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub id: String,
    #[serde(rename = "type")]
    pub version_type: Type,
    pub url: String,
    pub time: String,
    pub release_time: String,
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Type {
    OldAlpha,
    OldBeta,
    Release,
    Snapshot,
}

impl Deref for Type {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        match self {
            Type::OldAlpha => { "old_alpha" }
            Type::OldBeta => { "old_beta" }
            Type::Release => { "release" }
            Type::Snapshot => { "snapshot" }
        }
    }
}

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct VersionJson {
    #[serde(alias = "minecraftArguments")]
    pub arguments: Arguments,
    pub asset_index: Arc<AssetIndex>,
    pub assets: String,
    pub compliance_level: Option<i64>,
    pub downloads: Downloads,
    pub id: String,
    pub java_version: Option<JavaVersion>,
    pub logging: Option<Logging>,
    pub main_class: String,
    pub minimum_launcher_version: i64,
    pub release_time: String,
    pub time: String,
    #[serde(rename = "type")]
    pub release_type: Type,
    pub libraries: Arc<[Library]>,
}

impl VersionJson {
    /// Shorthand for matching and getting the ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Refers to the client jar url
    pub fn url(&self) -> &str {
        &self.downloads.client.url
    }

    /// Refers to the client jar sha1
    pub fn sha1(&self) -> &str {
        &self.downloads.client.sha1
    }

    pub fn libraries(&self) -> &Arc<[Library]> {
        &self.libraries
    }

    pub fn asset_index(&self) -> &Arc<AssetIndex> {
        &self.asset_index
    }

    pub fn release_type(&self) -> &Type {
        &self.release_type
    }

    pub fn main_class(&self) -> &str {
        &self.main_class
    }
}

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct AssetIndex {
    pub id: String,
    pub sha1: String,
    pub size: i64,
    pub total_size: Option<i64>,
    pub url: String,
}

#[skip_serializing_none]
#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Library {
    pub downloads: Option<Artifact>,
    pub name: String,
    pub rule: Rule,
    pub natives: Option<Natives>,
}

impl<'de> Deserialize<'de> for Library {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        pub struct TempLibrary {
            pub downloads: LibraryDownloads,
            pub name: String,
            pub rules: Option<Vec<Rule>>,
            pub extract: Option<Extract>,
            pub natives: Option<Natives>,
        }

        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        pub struct LibraryDownloads {
            pub artifact: Option<Artifact>,
            pub classifiers: Option<Classifiers>,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "kebab-case")]
        pub struct Classifiers {
            #[cfg_attr(target_arch = "x86_64", serde(alias = "linux-x86_64"))]
            pub natives_linux: Option<Artifact>,
            #[serde(alias = "natives_osx")]
            pub natives_macos: Option<Artifact>,
            #[cfg_attr(target_arch = "x86_64", serde(alias = "natives-windows-64"))]
            #[cfg_attr(target_arch = "x86", serde(alias = "natives-windows-32"))]
            pub natives_windows: Option<Artifact>,
        }

        let mut t = TempLibrary::deserialize(deserializer)?;

        let rule = if let Some(mut rules) = t.rules.take() {
            let idx = match rules.as_slice() {
                [rule_1, _] => {
                    if rule_1.os.is_some() && rule_1.action == Action::Disallow {
                        0
                    } else {
                        1
                    }
                }
                [_] => 0,
                _e => unreachable!("{_e:?}"),
            };

            rules.remove(idx)
        } else {
            Rule { action: Action::Allow, os: None }
        };

        let artifact = if let Some(mut classifier) = t.downloads.classifiers.take() {
            #[cfg(target_os = "windows")]
            {
                classifier.natives_windows.take()
            }

            #[cfg(target_os = "macos")]
            {
                classifier.natives_macos.take()
            }

            #[cfg(target_os = "linux")]
            {
                classifier.natives_linux.take()
            }
        } else {
            t.downloads.artifact.take()
        };

        Ok(Library {
            downloads: artifact,
            name: t.name,
            rule,
            natives: t.natives,
        })
    }
}

impl Natives {
    pub fn applies(&self) -> bool {
        #[cfg(windows)]
        return self.windows.is_some();
        #[cfg(target_os = "linux")]
        return self.linux.is_some();
        #[cfg(target_os = "mac_os")]
        return self.osx.is_some();
    }
}

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Natives {
    pub linux: Option<String>,
    pub osx: Option<String>,
    pub windows: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Extract {
    pub exclude: Vec<String>,
}

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub action: Action,
    pub os: Option<Os>,
}

impl Rule {
    pub fn apply(&self) -> bool {
        if let Some(os) = &self.os {
            os.name == OS && self.action == Action::Allow
                || os.name != OS && self.action == Action::Disallow
        } else {
            self.action == Action::Allow
        }
    }

    pub fn native(&self) -> bool {
        if let Some(os) = &self.os {
            os.name == OS && self.action == Action::Allow
                || os.name != OS && self.action == Action::Disallow
        } else {
            false
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum Action {
    Allow,
    Disallow,
}

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Os {
    pub name: OsName,
    pub version: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OsName {
    Windows,
    Linux,
    Osx,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub sha1: String,
    pub size: u64,
    pub url: String,
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetIndexJson {
    #[serde(skip_serializing, alias = "map_to_resources")]
    _map_to_resources: Option<bool>,
    #[serde(skip_serializing, alias = "virtual")]
    _virtual: Option<bool>,
    pub objects: std::collections::HashMap<String, Object>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Object {
    pub hash: String,
    pub size: u64,
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Arguments {
    pub game: Vec<GameElement>,
    pub jvm: Vec<JvmClass>,
}

impl<'de> Deserialize<'de> for Arguments {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct TempArguments {
            pub game: Vec<GameElement>,
            pub jvm: Vec<TempJvmClass>,
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum TempArgs {
            Args(TempArguments),
            String(String),
        }

        pub struct TempJvmClass {
            pub rules: Option<Vec<JvmRule>>,
            pub value: Value,
        }

        impl<'de> serde::de::Deserialize<'de> for TempJvmClass {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                #[derive(Deserialize)]
                struct Temp {
                    pub rules: Option<Vec<JvmRule>>,
                    pub value: Value,
                }

                #[derive(Deserialize)]
                #[serde(untagged)]
                enum TempClass {
                    Class(Temp),
                    String(String),
                }

                let v = TempClass::deserialize(deserializer)?;

                Ok(match v {
                    TempClass::Class(c) => TempJvmClass {
                        value: c.value,
                        rules: c.rules,
                    },
                    TempClass::String(s) => TempJvmClass {
                        value: Value::String(s),
                        rules: None,
                    },
                })
            }
        }

        let v = TempArgs::deserialize(deserializer)?;

        let r = match v {
            TempArgs::Args(t) => {
                let jvm = t
                    .jvm
                    .into_iter()
                    .map(|mut j| {
                        let rule = if let Some(mut rules) = j.rules.take() {
                            let idx = match rules.as_slice() {
                                [rule_1, _] => {
                                    if rule_1.os.name.is_some() && rule_1.action == Action::Disallow
                                    {
                                        0
                                    } else {
                                        1
                                    }
                                }
                                [_] => 0,
                                _e => unreachable!("{_e:?}"),
                            };

                            Some(rules.remove(idx))
                        } else {
                            None
                        };

                        JvmClass {
                            value: j.value,
                            rules: rule,
                        }
                    })
                    .collect();

                t.game.iter().for_each(|g| {
                   if let GameElement::GameClass(g) = &g {
                       if let Some(r) = &g.rules {
                           for x in r {
                               if x.action == Action::Disallow {
                                   panic!()
                               }
                           }
                       }
                   }
                });

                Arguments { jvm, game: t.game }
            }
            TempArgs::String(s) => Arguments {
                jvm: vec![
                    "-Djava.library.path=${natives_directory}",
                    "-Djna.tmpdir=${natives_directory}",
                    "-Dorg.lwjgl.system.SharedLibraryExtractPath=${natives_directory}",
                    "-Dio.netty.native.workdir=${natives_directory}",
                    "-Dminecraft.launcher.brand=${launcher_name}",
                    "-Dminecraft.launcher.version=${launcher_version}",
                    "-cp",
                    "${classpath}",
                ]
                .into_iter()
                .map(|s| JvmClass {
                    rules: None,
                    value: Value::String(s.to_owned()),
                })
                .collect(),
                game: s
                    .split(' ')
                    .map(|s| GameElement::String(s.to_owned()))
                    .collect(),
            },
        };

        Ok(r)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
#[serde(deny_unknown_fields)]
pub enum GameElement {
    GameClass(GameClass),
    String(String),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameClass {
    pub rules: Option<Vec<GameRule>>,
    #[serde(deserialize_with = "string_or_seq_string")]
    pub value: Box<[String]>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameRule {
    pub action: Action,
    pub features: Features,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Features {
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_demo_user: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub has_custom_resolution: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub has_quick_plays_support: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_quick_play_singleplayer: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_quick_play_multiplayer: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_quick_play_realms: bool,
}

fn is_false(b: &bool) -> bool {
    !b
}

#[skip_serializing_none]
#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JvmClass {
    pub rules: Option<JvmRule>,
    pub value: Value,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Array(Box<[String]>),
    String(String),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JvmRule {
    pub action: Action,
    pub os: PurpleOs,
}

impl JvmRule {
    pub fn applies(&self) -> bool {
        if let Some(os) = &self.os.name {
            os == &OS && self.action == Action::Allow
        } else {
            self.action == Action::Allow
        }
    }
}

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PurpleOs {
    pub name: Option<OsName>,
    pub arch: Option<String>,
    pub version: Option<String>,
}

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Downloads {
    pub client: Jar,
    pub client_mappings: Option<Jar>,
    pub server: Option<Jar>,
    pub server_mappings: Option<Jar>,
    pub windows_server: Option<Jar>,
}

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Jar {
    pub sha1: String,
    pub size: u64,
    pub url: String,
    pub path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct JavaVersion {
    pub component: String,
    pub major_version: i64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Logging {
    pub client: LoggingClient,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoggingClient {
    pub argument: String,
    pub file: AssetIndex,
    #[serde(rename = "type")]
    pub client_type: String,
}

use crate::OS;

fn string_or_seq_string<'de, D>(deserializer: D) -> Result<Box<[String]>, D::Error>
where
    D: Deserializer<'de>,
{
    struct StringOrBoxArray(std::marker::PhantomData<Box<[String]>>);

    impl<'de> serde::de::Visitor<'de> for StringOrBoxArray {
        type Value = Box<[String]>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("string or list of strings")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Box::new([value.to_owned()]))
        }

        fn visit_seq<S>(self, visitor: S) -> Result<Self::Value, S::Error>
        where
            S: serde::de::SeqAccess<'de>,
        {
            Deserialize::deserialize(serde::de::value::SeqAccessDeserializer::new(visitor))
        }
    }

    deserializer.deserialize_any(StringOrBoxArray(std::marker::PhantomData))
}
