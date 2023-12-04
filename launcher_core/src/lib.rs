extern crate core;

use std::fmt::Display;
use std::{path::Path, sync::atomic::AtomicUsize};

use crate::account::types::Account;
use futures::{stream, TryStreamExt};
use time::format_description::well_known::Rfc2822;

#[cfg(windows)]
const OS: &str = "windows";

#[cfg(target_os = "macos")]
const OS: &str = "osx";

#[cfg(target_os = "linux")]
const OS: &str = "linux";

pub mod account;
pub mod types;

#[derive(Clone)]
pub struct AsyncLauncher {
    client: reqwest::Client,
}

#[derive(Debug)]
pub enum Error {
    Reqwest(reqwest::Error),
    Tokio(tokio::io::Error),
    SerdeJson(serde_json::Error),
}

impl From<reqwest::Error> for Error {
    fn from(value: reqwest::Error) -> Self {
        Error::Reqwest(value)
    }
}

impl From<tokio::io::Error> for Error {
    fn from(value: tokio::io::Error) -> Self {
        Error::Tokio(value)
    }
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Error::SerdeJson(value)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            Error::Reqwest(err) => err.to_string(),
            Error::Tokio(err) => err.to_string(),
            Error::SerdeJson(err) => err.to_string(),
        };
        write!(f, "{}", str)
    }
}

impl serde::Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.to_string().as_str())
    }
}

impl AsyncLauncher {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    pub async fn get_version_manifest(
        &self,
        directory: &Path,
    ) -> Result<types::VersionManifest, Error> {
        const VERSION_MANIFEST_URL: &str =
            "https://launchermeta.mojang.com/mc/game/version_manifest.json";

        let file = directory.join("version_manifest.json");

        if tokio::fs::try_exists(&file).await? {
            let metadata = tokio::fs::metadata(&file).await?;
            let dt_mod = time::OffsetDateTime::from(metadata.modified()?);

            let response = self.client.head(VERSION_MANIFEST_URL).send().await?;
            let cdn_modified = response.headers()[reqwest::header::LAST_MODIFIED]
                .to_str()
                .unwrap();
            let dt_cdn = time::OffsetDateTime::parse(cdn_modified, &Rfc2822).unwrap();

            if dt_cdn < dt_mod {
                let buf = tokio::fs::read(file).await?;
                return Ok(serde_json::from_slice(&buf)?);
            }
        } else if !tokio::fs::try_exists(directory).await? {
            tokio::fs::create_dir_all(directory).await?;
        }

        let response = self.client.get(VERSION_MANIFEST_URL).send().await?;
        let bytes = response.bytes().await?;

        tokio::fs::write(file, &bytes).await?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// This expects a path such as `./Versions`
    pub async fn get_version_json(
        &self,
        version_details: &types::Version,
        directory: &Path,
    ) -> Result<types::VersionJson, Error> {
        let directory = directory.join(&version_details.id);
        let file = directory.join(format!("{}.json", version_details.id));

        if tokio::fs::try_exists(&file).await? {
            let buf = tokio::fs::read(file).await?;
            return Ok(serde_json::from_slice(&buf)?);
        }

        let response = self.client.get(&version_details.url).send().await?;
        let buf = response.bytes().await?;

        if !tokio::fs::try_exists(&directory).await? {
            tokio::fs::create_dir_all(&directory).await?;
        }
        tokio::fs::write(file, &buf).await?;

        let val = serde_json::from_slice(&buf)?;
        Ok(val)
    }

    /// This expects a top level path, ie: "./Assets", and will append /indexes/ to the end to store them
    pub async fn get_asset_index_json(
        &self,
        asset_index: &types::AssetIndex,
        directory: &Path,
    ) -> Result<types::AssetIndexJson, Error> {
        let directory = directory.join("indexes");
        let file = directory.join(format!("{}.json", asset_index.id));

        if tokio::fs::try_exists(&file).await? {
            let buf = tokio::fs::read(&file).await?;
            let val = serde_json::from_slice(&buf)?;
            Ok(val)
        } else {
            let response = self.client.get(&asset_index.url).send().await?;
            let buf = response.bytes().await?;

            if !tokio::fs::try_exists(&directory).await? {
                tokio::fs::create_dir_all(&directory).await?;
            }

            tokio::fs::write(file, &buf).await?;

            let val = serde_json::from_slice(&buf)?;
            Ok(val)
        }
    }

    /// This expects a top level path, ie: "./Assets", and will append /objects/ to the end to store them
    pub async fn download_and_store_asset_index(
        &self,
        asset_index: &types::AssetIndexJson,
        directory: &Path,
        total: &AtomicUsize,
        finished: &AtomicUsize,
    ) -> Result<(), Error> {
        const ASSET_BASE_URL: &str = "https://resources.download.minecraft.net";

        total.store(
            asset_index.objects.len(),
            std::sync::atomic::Ordering::Relaxed,
        );

        finished.store(0, std::sync::atomic::Ordering::Relaxed);

        let object_path = directory.join("objects");
        if !tokio::fs::try_exists(&object_path).await? {
            tokio::fs::create_dir(&object_path).await?;
        }

        stream::iter(asset_index.objects.values().map(Ok))
            .try_for_each_concurrent(16, |asset| {
                let client = self.client.clone();
                let mut sha1 = sha1_smol::Sha1::new();
                let directory = &object_path;
                async move {
                    let first_two = &asset.hash[0..2];
                    let dir_path = directory.join(first_two);
                    let file_path = dir_path.join(&asset.hash);

                    if !tokio::fs::try_exists(&dir_path).await? {
                        tokio::fs::create_dir_all(dir_path).await?;
                    }

                    let url = if tokio::fs::try_exists(&file_path).await? {
                        let buf = tokio::fs::read(&file_path).await?;
                        sha1.update(&buf);

                        let digest = sha1.digest().to_string();

                        if digest != asset.hash {
                            tokio::fs::remove_file(&file_path).await?;
                            format!("{}/{}/{}/", ASSET_BASE_URL, first_two, &asset.hash)
                        } else {
                            finished.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            return Ok(());
                        }
                    } else {
                        format!("{}/{}/{}", ASSET_BASE_URL, first_two, &asset.hash)
                    };

                    let response = client.get(url).send().await?;
                    let bytes = response.bytes().await?;
                    tokio::fs::write(&file_path, bytes).await?;

                    finished.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    Ok(())
                }
            })
            .await
    }

    pub async fn download_libraries_and_get_path(
        &self,
        libraries: &[types::Library],
        directory: &Path,
        native_dir: &Path,
        total: &AtomicUsize,
        finished: &AtomicUsize,
    ) -> Result<String, Error> {
        let mut path = String::new();

        finished.store(0, std::sync::atomic::Ordering::Relaxed);

        stream::iter(libraries.iter().filter_map(|library| {
            let dir = directory.to_str().unwrap();
            path.reserve(library.name.len() + dir.len() + 2);
            let mut lib: Option<&types::Library> = None;
            if let Some(rules) = &library.rules {
                for rule in rules {
                    if let Some(os) = &rule.os {
                        if os.name == OS && rule.action == types::Action::Allow {
                            lib = Some(library);
                        } else {
                            return None;
                        }
                    } else if rule.action == types::Action::Allow {
                        lib = Some(library);
                    } else {
                        return None;
                    }
                }
            } else {
                lib = Some(library);
            }

            // This guarantees that it is initialized
            // Because if it's none here then there
            // Is no library to check to begin with
            let Some(lib) = lib else {
                return None;
            };

            let artifact = if let Some(artifact) = &lib.downloads.artifact {
                (lib.natives.is_some() || lib.extract.is_some(), artifact)
            } else if let Some(classifier) = &library.downloads.classifiers {
                #[cfg(target_os = "windows")]
                let option = if classifier.natives_windows.is_some() {
                    classifier.natives_windows.as_ref()
                } else if cfg!(target_arch = "x86_64") {
                    classifier.natives_windows_64.as_ref()
                } else if cfg!(target_arch = "x86") {
                    classifier.natives_windows_32.as_ref()
                } else {
                    classifier.natives_windows.as_ref()
                };

                #[cfg(target_os = "macos")]
                let option = if classifier.natives_osx.is_some() {
                    classifier.natives_osx.as_ref()
                } else {
                    classifier.natives_macos.as_ref()
                };

                #[cfg(target_os = "linux")]
                let option = if classifier.natives_linux.is_some() {
                    classifier.natives_linux.as_ref()
                } else if cfg!(target_arch = "x86_64") {
                    classifier.linux_x86_64.as_ref()
                } else {
                    classifier.natives_linux.as_ref()
                };

                match option {
                    Some(art) => (true, art),
                    None => return None,
                }
            } else {
                unreachable!("Found missing artifact")
            };

            #[cfg(not(windows))]
            path.extend([dir, "/", &artifact.1.path, ":"]);

            #[cfg(windows)]
            path.extend([dir, "/", &artifact.1.path, ";"]);

            total.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

            assert_eq!(lib.natives.is_some() || lib.extract.is_some(), artifact.0, "{}", lib.name);

            Some(Ok::<_, Error>(artifact))
        }))
        .try_for_each_concurrent(16, |(native, artifact)| {
            let client = self.client.clone();
            let mut sha1 = sha1_smol::Sha1::new();
            let path = directory.join(Path::new(&artifact.path));
            let url = &artifact.url;
            let finished = &finished;

            async move {
                let parent = path.parent().unwrap();

                if tokio::fs::try_exists(&path).await? {
                    let buf = tokio::fs::read(&path).await?;
                    sha1.update(&buf);
                    if sha1.digest().to_string() == artifact.sha1 {
                        finished.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                        if native {
                            extract_native(native_dir, buf).await?;
                        }

                        return Ok(());
                    } else {
                        tokio::fs::remove_file(&path).await?;
                    }
                }

                let response = client.get(url).send().await?;
                let bytes = response.bytes().await?;
                tokio::fs::create_dir_all(parent).await?;

                if native {
                    extract_native(native_dir, bytes.to_vec()).await?;
                    tokio::fs::write(path, bytes).await?;
                } else {
                    tokio::fs::write(path, bytes).await?;
                }

                finished.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Ok(())
            }
        })
        .await?;

        Ok(path)
    }

    pub async fn download_jar(
        &self,
        version_details: &types::VersionJson,
        directory: &Path,
    ) -> Result<String, Error> {
        let id = version_details.id();
        let url = version_details.url();
        let folder = directory.join(id);

        let file = folder.join(format!("{id}.jar"));
        let str = file.to_str().unwrap().to_string();

        if tokio::fs::try_exists(&file).await? {
            let mut hasher = sha1_smol::Sha1::new();
            let buf = tokio::fs::read(&file).await?;
            hasher.update(&buf);
            if hasher.digest().to_string() == version_details.sha1() {
                return Ok(str);
            }
        }

        let jar = self.client.get(url).send().await?;
        let buf = jar.bytes().await?;
        tokio::fs::write(file, buf).await?;

        Ok(str)
    }
}

async fn extract_native(native_dir: &Path, buf: Vec<u8>) -> Result<(), Error> {
    if !tokio::fs::try_exists(native_dir).await? {
        tokio::fs::create_dir_all(native_dir).await?;
    }

    let reader = async_zip::base::read::mem::ZipFileReader::new(buf).await.unwrap();
    'outer: for index in 0..reader.file().entries().len() {
        use tokio_util::compat::FuturesAsyncReadCompatExt;

        let mut entry_reader = reader.reader_without_entry(index).await.unwrap().compat();
        let entry = reader.file().entries().get(index).unwrap().entry();
        if entry.dir().unwrap() {
            continue;
        } else {
            let file = entry.filename().as_str().unwrap();

            #[cfg(windows)]
            let ends_with = ".dll";
            #[cfg(target_os = "linux")]
            let ends_with = ".so";
            #[cfg(target_os = "macos")]
            let ends_with = ".dylib";

            if !file.ends_with(ends_with) {
                continue 'outer;
            }

            let mut buffer = Vec::new();
            tokio::io::copy(&mut entry_reader, &mut buffer).await?;
            tokio::fs::write(native_dir.join(file), &buffer).await?;
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn launch_modern_version(
    json: &types::Modern,
    directory: &Path,
    asset_root: &Path,

    account: &Account,
    client_id: &str,
    auth_xuid: &str,

    launcher_name: &str,
    launcher_version: &str,
    class_path: &str,
) {
    let mut process = std::process::Command::new("java");
    let args = &json.arguments;
    let natives_dir = directory.join("natives");

    args.jvm.iter().for_each(|arg| match arg {
        types::modern::JvmElement::JvmClass(class) => {
            class.rules.iter().for_each(|rule| {
                if let Some(os) = &rule.os.name {
                    if rule.action == types::Action::Allow && os == OS {
                        match &class.value {
                            types::modern::Value::String(arg) => {
                                process.arg(arg);
                            }
                            types::modern::Value::StringArray(args) => {
                                for arg in args {
                                    process.arg(arg);
                                }
                            }
                        }
                    }
                }
            });
        }
        types::modern::JvmElement::String(arg) => {
            let arg = arg
                .replace("${natives_directory}", &natives_dir.to_string_lossy())
                .replace("${launcher_name}", launcher_name)
                .replace("${launcher_version}", launcher_version)
                .replace("${classpath}", class_path);

            process.arg(arg);
        }
    });

    process.arg(&json.main_class);

    for arg in &args.game {
        match arg {
            types::modern::GameElement::GameClass(_) => {
                // This is left empty, as I have not setup support for any of the features here
            }
            types::modern::GameElement::String(arg) => {
                let arg = arg
                    .replace("${auth_player_name}", &account.profile.name)
                    .replace("${version_name}", &json.id)
                    .replace("${game_directory}", &directory.to_string_lossy())
                    .replace("${assets_root}", &asset_root.to_string_lossy())
                    .replace("${assets_index_name}", &json.asset_index.id)
                    .replace("${auth_uuid}", &account.profile.id)
                    .replace("${auth_access_token}", &account.access_token)
                    .replace("${clientid}", client_id)
                    .replace("${auth_xuid}", auth_xuid)
                    .replace("${user_type}", "msa")
                    .replace("${version_type}", &json.welcome_type);

                process.arg(arg);
            }
        }
    }

    process.spawn().unwrap();
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path, sync::atomic::AtomicUsize};

    use reqwest::Client;

    use crate::{types::VersionJson, AsyncLauncher};

    #[tokio::test]
    async fn test_version_types() {
        let launcher = AsyncLauncher::new(Client::new());
        let manifest = launcher
            .get_version_manifest(Path::new("./Versions"))
            .await
            .unwrap();
        let _ = fs::create_dir("./Versions");
        for version in manifest.versions.iter() {
            if let Err(err) = launcher
                .get_version_json(version, Path::new("./Versions"))
                .await
            {
                println!("{}", version.id);
                println!("{:?}", err);
            }
        }
    }

    #[tokio::test]
    async fn test_assets() {
        let launcher = AsyncLauncher::new(Client::new());
        let manifest = launcher
            .get_version_manifest(Path::new("./Versions"))
            .await
            .unwrap();
        fs::create_dir("./Assets").unwrap();
        if let Ok(VersionJson::Modern(version)) = launcher
            .get_version_json(&manifest.versions[0], Path::new("./Versions"))
            .await
        {
            if let Ok(index) = launcher
                .get_asset_index_json(&version.asset_index, Path::new("./Assets"))
                .await
            {
                if let Err(err) = launcher
                    .download_and_store_asset_index(
                        &index,
                        Path::new("./Assets"),
                        &AtomicUsize::new(0),
                        &AtomicUsize::new(0),
                    )
                    .await
                {
                    panic!("{:?}", err)
                }
            }
        }
    }

    #[tokio::test]
    async fn test_libs() {
        let launcher = AsyncLauncher::new(Client::new());
        let manifest = launcher
            .get_version_manifest(Path::new("./Versions"))
            .await
            .unwrap();
        // fs::create_dir("./Libs").unwrap();
        for version in &manifest.versions {
            let libs = match launcher
                .get_version_json(version, Path::new("./Versions"))
                .await
            {
                Ok(version) => version.libraries().clone(),
                Err(err) => panic!("How {err:?}"),
            };
            // println!("{:?}", version.id);
            if let Err(e) = launcher
                .download_libraries_and_get_path(
                    &libs,
                    Path::new("./Libs"),
                    Path::new("./natives"),
                    &AtomicUsize::new(0),
                    &AtomicUsize::new(0),
                )
                .await {
                println!("{} {e:?}", version.id)
            };
            // println!("{}", path);
        }
        fs::remove_dir_all("./Libs").unwrap();
    }
}
