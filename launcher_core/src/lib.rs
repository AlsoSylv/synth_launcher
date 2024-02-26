use std::fmt::Display;
use std::path::Path;
use std::sync::atomic::AtomicU64;

use crate::account::types::Account;
use crate::types::{OsName, Value};
use futures::{stream, Stream, StreamExt, TryStreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::bytes;
use tokio_util::compat::FuturesAsyncReadCompatExt;

#[cfg(windows)]
const OS: OsName = OsName::Windows;

#[cfg(target_os = "macos")]
const OS: OsName = OsName::Osx;

#[cfg(target_os = "linux")]
const OS: OsName = OsName::Linux;

pub mod account;
pub mod types;

#[derive(Clone)]
pub struct AsyncLauncher {
    client: reqwest::Client,
}

#[derive(Debug)]
#[repr(u32)]
pub enum Error {
    Reqwest(reqwest::Error),
    Tokio(tokio::io::Error),
    SerdeJson(serde_json::Error),
    ProfileError(account::types::ProfileError),
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
        let str: &dyn Display = match self {
            Error::Reqwest(err) => err,
            Error::Tokio(err) => err,
            Error::SerdeJson(err) => err,
            Error::ProfileError(err) => err,
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

    /// Downloads "version_manifest.json" to the provided directory,
    /// Returning a copy in memory. This will automatically append
    /// new entries to the start of the version manifest.
    ///
    /// # Example
    ///
    /// ```
    /// async fn example() {
    ///     let client  = reqwest::Client::new();
    ///     let launcher = launcher_core::AsyncLauncher::new(client);
    ///
    ///     let path = std::path::Path::new("./");
    ///     launcher.get_version_manifest(path).await.unwrap();
    /// }
    pub async fn get_version_manifest(
        &self,
        directory: &Path,
    ) -> Result<types::VersionManifest, Error> {
        const VERSION_MANIFEST_URL: &str =
            "https://launchermeta.mojang.com/mc/game/version_manifest.json";

        let file = directory.join("version_manifest.json");

        if tokio::fs::try_exists(&file).await? {
            let response = self.client.get(VERSION_MANIFEST_URL).send().await?;

            let buf = tokio::fs::read(&file).await?;
            let mut meta: types::VersionManifest = serde_json::from_slice(&buf)?;

            let mut updated: types::VersionManifest = response.json().await?;

            if meta.latest != updated.latest {
                updated
                    .versions
                    .drain(..)
                    .filter(|v| !meta.versions.contains(v))
                    .collect::<Vec<types::Version>>()
                    .drain(..)
                    .enumerate()
                    .for_each(|(idx, version)| meta.versions.insert(idx, version));
            }

            let slice = serde_json::to_vec(&meta)?;

            tokio::fs::write(file, &slice).await?;

            Ok(meta)
        } else {
            if !tokio::fs::try_exists(directory).await? {
                tokio::fs::create_dir_all(directory).await?;
            }

            let response = self.client.get(VERSION_MANIFEST_URL).send().await?;
            let bytes = response.bytes().await?;

            tokio::fs::write(file, &bytes).await?;
            Ok(serde_json::from_slice(&bytes)?)
        }
    }

    /// This expects a path such as `./Versions`
    pub async fn get_version_json(
        &self,
        version_details: &types::Version,
        directory: &Path,
    ) -> Result<types::VersionJson, Error> {
        let directory = directory.join(&version_details.id);
        let file = directory.join(format!("{}.json", version_details.id));
        #[cfg(debug_assertions)]
        let trans = directory.join(format!("trans_{}.json", version_details.id));

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

        #[cfg(debug_assertions)]
        tokio::fs::write(trans, &serde_json::to_vec_pretty(&val)?).await?;

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

            if sha1(&buf) == asset_index.sha1 {
                let val = serde_json::from_slice(&buf)?;
                return Ok(val);
            }
        }
        let response = self.client.get(&asset_index.url).send().await?;
        let buf = response.bytes().await?;

        if !tokio::fs::try_exists(&directory).await? {
            tokio::fs::create_dir_all(&directory).await?;
        }

        tokio::fs::write(file, &buf).await?;

        let val = serde_json::from_slice(&buf)?;
        Ok(val)
    }

    /// This expects a top level path, ie: "./Assets", and will append /objects/ to the end to store them
    pub async fn download_and_store_asset_index(
        &self,
        asset_index: &types::AssetIndexJson,
        directory: &Path,
        total: &AtomicU64,
        finished: &AtomicU64,
    ) -> Result<(), Error> {
        const ASSET_BASE_URL: &str = "https://resources.download.minecraft.net";

        total.store(
            asset_index
                .objects
                .values()
                .fold(0, |acc, obj| acc + obj.size),
            std::sync::atomic::Ordering::Relaxed,
        );
        finished.store(0, std::sync::atomic::Ordering::Relaxed);

        let object_path = &directory.join("objects");
        if !tokio::fs::try_exists(&object_path).await? {
            tokio::fs::create_dir(&object_path).await?;
        }

        stream::iter(asset_index.objects.values().map(Ok))
            .try_for_each_concurrent(16, |asset| async {
                let first_two = &asset.hash[0..2];
                let dir_path = object_path.join(first_two);
                let file_path = dir_path.join(&asset.hash);

                if file_path.exists() {
                    let mut buf = [0; 64 * 1024];
                    let mut file = tokio::fs::File::open(&file_path).await?;
                    let mut hasher = sha1_smol::Sha1::new();

                    let mut total_read = 0;
                    loop {
                        let read_bytes = file.read(&mut buf).await?;
                        total_read += read_bytes;
                        hasher.update(&buf[..read_bytes]);
                        if total_read == asset.size as usize {
                            break;
                        }
                    }

                    let hash = hasher.digest().to_string();

                    if hasher.digest().to_string() == asset.hash {
                        finished.fetch_add(asset.size, std::sync::atomic::Ordering::Relaxed);
                        return Ok(());
                    } else {
                        println!(
                            "Hash was wrong, expected: {}, but found: {hash}",
                            asset.hash
                        );
                        tokio::fs::remove_file(&file_path).await?;
                    }
                } else if !dir_path.exists() {
                    tokio::fs::create_dir_all(dir_path).await?;
                };

                let url = format!("{}/{}/{}", ASSET_BASE_URL, first_two, &asset.hash);
                let response = self.client.get(url).send().await?;
                let mut bytes = response.bytes_stream();
                let mut file = tokio::fs::File::create(&file_path).await?;

                while let Some(chunk) = bytes.next().await {
                    let chunk = chunk.unwrap();
                    file.write_all(&chunk).await?;
                    finished.fetch_add(chunk.len() as u64, std::sync::atomic::Ordering::Relaxed);
                }

                Ok(())
            })
            .await
    }

    pub async fn download_libraries_and_get_path(
        &self,
        libraries: &[types::Library],
        directory: &Path,
        native_dir: &Path,
        total: &AtomicU64,
        finished: &AtomicU64,
    ) -> Result<String, Error> {
        let mut path = String::new();

        finished.store(0, std::sync::atomic::Ordering::Relaxed);
        total.store(0, std::sync::atomic::Ordering::Relaxed);

        stream::iter(libraries.iter().filter_map(|library| {
            let native = library.rule.native();

            let Some(artifact) = &library.downloads else {
                return None;
            };

            if !library.rule.apply() {
                return None;
            }

            let dir = directory.to_str().unwrap();
            #[cfg(not(windows))]
            path.extend([dir, "/", &artifact.path, ":"]);

            #[cfg(windows)]
            path.extend([dir, "/", &artifact.path, ";"]);

            total.fetch_add(artifact.size, std::sync::atomic::Ordering::Relaxed);

            Some(Ok::<_, Error>((artifact, native)))
        }))
        .try_for_each_concurrent(16, |(artifact, native)| async move {
            let mut fetch = true;

            let path = directory.join(Path::new(&artifact.path));
            let parent = path.parent().unwrap();

            if path.exists() {
                let buf = tokio::fs::read(&path).await?;
                if sha1(&buf) == artifact.sha1 {
                    fetch = false;
                } else {
                    tokio::fs::remove_file(&path).await?;
                }
            }

            if fetch {
                tokio::fs::create_dir_all(parent).await?;

                let response = self.client.get(&artifact.url).send().await?;
                let mut stream = response.bytes_stream();
                let mut file = tokio::fs::File::create(&path).await?;
                write_file(&mut file, &mut stream, finished).await?;
            } else {
                finished.fetch_add(artifact.size, std::sync::atomic::Ordering::Relaxed);
            }

            if native {
                extract_native(native_dir, &path).await
            } else {
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
        total_bytes: &AtomicU64,
        finished_bytes: &AtomicU64,
    ) -> Result<String, Error> {
        total_bytes.store(
            version_details.downloads.client.size,
            std::sync::atomic::Ordering::Relaxed,
        );
        finished_bytes.store(0, std::sync::atomic::Ordering::Relaxed);

        let id = version_details.id();
        let url = version_details.url();
        let folder = directory.join(id);

        let file = folder.join(format!("{id}.jar"));
        let str = file.to_str().unwrap().to_string();

        if tokio::fs::try_exists(&file).await? {
            let buf = tokio::fs::read(&file).await?;
            if sha1(&buf) == version_details.sha1() {
                finished_bytes.store(version_details.downloads.client.size, std::sync::atomic::Ordering::Relaxed);
                return Ok(str);
            }
        }

        let mut file = tokio::fs::File::create(file).await?;

        let jar = self.client.get(url).send().await?;
        let len = jar.content_length().unwrap();
        finished_bytes.store(len, std::sync::atomic::Ordering::Relaxed);

        let mut stream = jar.bytes_stream();
        write_file(&mut file, &mut stream, finished_bytes).await?;

        Ok(str)
    }
}

async fn write_file<S>(
    file: &mut tokio::fs::File,
    stream: &mut S,
    bytes: &AtomicU64,
) -> Result<(), Error>
where
    S: Stream<Item = reqwest::Result<bytes::Bytes>> + Unpin,
{
    while let Some(next) = stream.next().await {
        let chunk = next?;
        file.write_all(&chunk).await?;
        bytes.fetch_add(chunk.len() as u64, std::sync::atomic::Ordering::Relaxed);
    }

    Ok(())
}

async fn extract_native(native_dir: &Path, path: &Path) -> Result<(), Error> {
    if !tokio::fs::try_exists(native_dir).await? {
        tokio::fs::create_dir_all(native_dir).await?;
    }

    let reader = async_zip::tokio::read::fs::ZipFileReader::new(path)
        .await
        .unwrap();
    for (idx, entry) in reader.file().entries().iter().enumerate() {
        if entry.dir().unwrap() {
            continue;
        }
        let file_path = entry.filename().as_str().unwrap();

        #[cfg(windows)]
        let ends_with = ".dll";
        #[cfg(target_os = "linux")]
        let ends_with = ".so";
        #[cfg(target_os = "macos")]
        let ends_with = ".dylib";

        if !file_path.ends_with(ends_with) {
            continue;
        }

        let file = file_path.split('/').last().unwrap();
        let mut entry_reader = reader.reader_without_entry(idx).await.unwrap().compat();
        let mut buffer = Vec::with_capacity(entry.uncompressed_size() as usize);
        tokio::io::copy(&mut entry_reader, &mut buffer).await?;
        tokio::fs::write(native_dir.join(file), &buffer).await?;
    }

    Ok(())
}

fn sha1(buf: &[u8]) -> String {
    let mut sha1 = sha1_smol::Sha1::new();
    sha1.update(buf);
    sha1.digest().to_string()
}

#[allow(clippy::too_many_arguments)]
pub fn launch_game(
    java_path: &str,
    json: &types::VersionJson,
    directory: &Path,
    asset_root: &Path,

    account: &Account,
    client_id: &str,
    auth_xuid: &str,

    launcher_name: &str,
    launcher_version: &str,
    class_path: &str,
) {
    let mut process = std::process::Command::new(java_path);
    let natives_dir = directory.join("natives");

    for arg in &json.arguments.jvm {
        if let Some(rules) = &arg.rules {
            if !rules.applies() {
                continue;
            }
        }

        match &arg.value {
            Value::Array(arr) => {
                arr.iter().for_each(|s| {
                    let arg = apply_jvm_args(
                        s,
                        &natives_dir,
                        launcher_name,
                        launcher_version,
                        class_path,
                    );
                    process.arg(arg);
                });
            }
            Value::String(s) => {
                let arg =
                    apply_jvm_args(s, &natives_dir, launcher_name, launcher_version, class_path);
                process.arg(arg);
            }
        }
    }

    process.arg(json.main_class());

    for arg in &json.arguments.game {
        match &arg {
            types::GameElement::GameClass(_) => {
                // This is left empty, as I have not setup support for any of the features here
            }
            types::GameElement::String(arg) => {
                let arg = apply_mc_args(
                    arg, json, directory, asset_root, account, client_id, auth_xuid,
                );

                process.arg(arg);
            }
        }
    }

    process.spawn().unwrap();
}

fn apply_jvm_args(
    string: &str,
    natives_dir: &Path,
    launcher_name: &str,
    launcher_version: &str,
    class_path: &str,
) -> String {
    string
        .replace("${natives_directory}", &natives_dir.to_string_lossy())
        .replace("${launcher_name}", launcher_name)
        .replace("${launcher_version}", launcher_version)
        .replace("${classpath}", class_path)
}

fn apply_mc_args(
    string: &str,
    json: &types::VersionJson,
    directory: &Path,
    asset_root: &Path,

    account: &Account,
    client_id: &str,
    auth_xuid: &str,
) -> String {
    string
        .replace("${auth_player_name}", &account.profile.name)
        .replace("${version_name}", json.id())
        .replace("${game_directory}", &directory.to_string_lossy())
        .replace("${assets_root}", &asset_root.to_string_lossy())
        .replace("${game_assets}", &asset_root.to_string_lossy())
        .replace("${assets_index_name}", &json.asset_index().id)
        .replace("${auth_uuid}", &account.profile.id)
        .replace("${auth_access_token}", &account.access_token)
        .replace("${auth_session}", &account.access_token)
        .replace("${clientid}", client_id)
        .replace("${auth_xuid}", auth_xuid)
        .replace("${user_properties}", "{}")
        .replace("${user_type}", "msa")
        .replace("${version_type}", json.release_type())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU64;
    use std::{fs, path::Path};

    use reqwest::Client;
    use tokio::io::AsyncWriteExt;

    use crate::AsyncLauncher;

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
        if let Ok(version) = launcher
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
                        &AtomicU64::new(0),
                        &AtomicU64::new(0),
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
        let _ = fs::create_dir("./Libs");
        for version in &manifest.versions {
            let libs = match launcher
                .get_version_json(version, Path::new("./Versions"))
                .await
            {
                Ok(version) => version,
                Err(err) => panic!("How {err:?}"),
            };
            // println!("{:?}", version.id);
            if let Err(e) = launcher
                .download_libraries_and_get_path(
                    libs.libraries(),
                    Path::new("./Libs"),
                    Path::new("./natives"),
                    &AtomicU64::new(0),
                    &AtomicU64::new(0),
                )
                .await
            {
                println!("{} {e:?}", version.id)
            };
            // println!("{}", path);
        }
    }

    #[tokio::test]
    async fn stream_write() {
        use futures::stream::StreamExt;
        let client = Client::new();

        let mut file = tokio::fs::File::create("./1.20.3.jar").await.unwrap();

        let jar = client.get("https://piston-data.mojang.com/v1/objects/b178a327a96f2cf1c9f98a45e5588d654a3e4369/client.jar").send().await.unwrap();

        let mut stream = jar.bytes_stream();
        while let Some(next) = stream.next().await {
            let chunk = next.unwrap();
            file.write_all(&chunk).await.unwrap();
        }
    }

    #[tokio::test]
    async fn batch_write() {
        let client = Client::new();

        let jar = client.get("https://piston-data.mojang.com/v1/objects/b178a327a96f2cf1c9f98a45e5588d654a3e4369/client.jar").send().await.unwrap();

        let stream = jar.bytes().await.unwrap();

        tokio::fs::write("./1.20.3.jar", &stream).await.unwrap();
    }
}
