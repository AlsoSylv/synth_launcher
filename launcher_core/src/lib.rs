extern crate core;

use std::fmt::Display;
use std::{path::Path, sync::atomic::AtomicUsize};

use crate::account::types::Account;
use futures::{stream, StreamExt, TryStreamExt};
use tokio::io::AsyncWriteExt;

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

            let updated: types::VersionManifest = response.json().await?;

            if meta.latest != updated.latest {
                for versions in updated.versions {
                    if !meta.versions.contains(&versions) {
                        meta.versions.push(versions);
                    }
                }
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
            let mut sha1 = sha1_smol::Sha1::new();
            let buf = tokio::fs::read(&file).await?;

            sha1.update(&buf);

            if sha1.digest().to_string() == asset_index.sha1 {
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
        total.store(0, std::sync::atomic::Ordering::Relaxed);

        stream::iter(libraries.iter().filter_map(|library| {
            let mut native = library.natives.is_some() || library.extract.is_some();
            let dir = directory.to_str().unwrap();
            if let Some(rules) = &library.rules {
                let mut rule_iter = rules.iter();

                if !rule_iter.any(|rule| rule.applies()) {
                    return None;
                }

                native |= rule_iter.all(|rule| rule.native());
            }

            // Move to after rule validation to reduce
            path.reserve_exact(library.name.len() + dir.len() + 2);

            let artifact: &types::Artifact;

            if let Some(classifier) = &library.downloads.classifiers {
                let option: Option<&types::Artifact>;

                #[cfg(target_os = "windows")]
                if classifier.natives_windows.is_some() {
                    option = classifier.natives_windows.as_ref()
                } else if cfg!(target_arch = "x86_64") {
                    option = classifier.natives_windows_64.as_ref()
                } else if cfg!(target_arch = "x86") {
                    option = classifier.natives_windows_32.as_ref()
                } else {
                    option = classifier.natives_windows.as_ref()
                };

                #[cfg(target_os = "macos")]
                if classifier.natives_osx.is_some() {
                    option = classifier.natives_osx.as_ref()
                } else {
                    option = classifier.natives_macos.as_ref()
                };

                #[cfg(target_os = "linux")]
                if classifier.natives_linux.is_some() {
                    option = classifier.natives_linux.as_ref()
                } else if cfg!(target_arch = "x86_64") {
                    option = classifier.linux_x86_64.as_ref()
                } else {
                    option = classifier.natives_linux.as_ref()
                };

                match option {
                    Some(art) => artifact = art,
                    None => return None,
                }
            } else if let Some(art) = &library.downloads.artifact {
                artifact = art;
            } else {
                unreachable!("Found missing artifact")
            };

            #[cfg(not(windows))]
            path.extend([dir, "/", &artifact.path, ":"]);

            #[cfg(windows)]
            path.extend([dir, "/", &artifact.path, ";"]);

            Some(Ok::<_, Error>((native, artifact)))
        }))
        .try_for_each_concurrent(16, |(native, artifact)| {
            total.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

            let client = self.client.clone();
            let mut sha1 = sha1_smol::Sha1::new();
            let path = directory.join(Path::new(&artifact.path));
            let url = &artifact.url;

            async move {
                let parent = path.parent().unwrap();

                if tokio::fs::try_exists(&path).await? {
                    let buf = tokio::fs::read(&path).await?;
                    sha1.update(&buf);
                    if sha1.digest().to_string() == artifact.sha1 {
                        if native {
                            extract_native(native_dir, buf).await?;
                        }

                        finished.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

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
        total_bytes: &AtomicUsize,
        finished_bytes: &AtomicUsize,
    ) -> Result<String, Error> {
        total_bytes.store(0, std::sync::atomic::Ordering::Relaxed);
        finished_bytes.fetch_add(0, std::sync::atomic::Ordering::Relaxed);

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

        let mut file = tokio::fs::File::create(file).await?;

        let jar = self.client.get(url).send().await?;
        let len = jar.content_length().unwrap();
        total_bytes.store(len as usize, std::sync::atomic::Ordering::Relaxed);

        let mut stream = jar.bytes_stream();
        while let Some(next) = stream.next().await {
            let chunk = next?;
            file.write_all(&chunk).await?;
            finished_bytes.fetch_add(chunk.len(), std::sync::atomic::Ordering::Relaxed);
        }

        Ok(str)
    }
}

async fn extract_native(native_dir: &Path, buf: Vec<u8>) -> Result<(), Error> {
    if !tokio::fs::try_exists(native_dir).await? {
        tokio::fs::create_dir_all(native_dir).await?;
    }

    let reader = async_zip::base::read::mem::ZipFileReader::new(buf)
        .await
        .unwrap();
    for index in 0..reader.file().entries().len() {
        use tokio_util::compat::FuturesAsyncReadCompatExt;

        let mut entry_reader = reader.reader_with_entry(index).await.unwrap().compat();
        let entry = reader.file().entries().get(index).unwrap().entry();
        if entry.dir().unwrap() {
            continue;
        } else {
            let file_path = entry.filename().as_str().unwrap();
            let file = file_path.split('/').last().unwrap();

            #[cfg(windows)]
            let ends_with = ".dll";
            #[cfg(target_os = "linux")]
            let ends_with = ".so";
            #[cfg(target_os = "macos")]
            let ends_with = ".dylib";

            if !file.ends_with(ends_with) {
                continue;
            }

            let mut buffer = Vec::new();
            tokio::io::copy(&mut entry_reader, &mut buffer).await?;
            tokio::fs::write(native_dir.join(file), &buffer).await?;
        }
    }

    Ok(())
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

    if let Some(args) = &json.arguments {
        args.jvm.iter().for_each(|arg| match arg {
            types::JvmElement::JvmClass(class) => {
                class.rules.iter().for_each(|rule| {
                    if let Some(os) = &rule.os.name {
                        if rule.action == types::Action::Allow && os == OS {
                            match &class.value {
                                types::Value::String(arg) => {
                                    process.arg(arg);
                                }
                                types::Value::StringArray(args) => {
                                    for arg in args {
                                        process.arg(arg);
                                    }
                                }
                            }
                        }
                    }
                });
            }
            types::JvmElement::String(arg) => {
                let arg = apply_jvm_args(
                    arg,
                    &natives_dir,
                    launcher_name,
                    launcher_version,
                    class_path,
                );

                process.arg(arg);
            }
        });

        process.arg(json.main_class());

        for arg in &args.game {
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
    } else {
        let jvm_args = [
            "-Djava.library.path=${natives_directory}",
            "-Djna.tmpdir=${natives_directory}",
            "-Dorg.lwjgl.system.SharedLibraryExtractPath=${natives_directory}",
            "-Dio.netty.native.workdir=${natives_directory}",
            "-Dminecraft.launcher.brand=${launcher_name}",
            "-Dminecraft.launcher.version=${launcher_version}",
            "-cp",
            "${classpath}",
        ];

        for arg in jvm_args {
            let arg = apply_jvm_args(
                arg,
                &natives_dir,
                launcher_name,
                launcher_version,
                class_path,
            );

            process.arg(arg);
        }

        process.arg(json.main_class());

        if let Some(args) = &json.minecraft_arguments {
            let args: Vec<&str> = args.split(' ').collect();
            for arg in args {
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
    use std::{fs, path::Path, sync::atomic::AtomicUsize};

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
