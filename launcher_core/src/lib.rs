use std::path::Path;

use futures::{stream, TryStreamExt};

#[cfg(windows)]
const OS: &str = "windows";

#[cfg(target_os = "macos")]
const OS: &str = "osx";

#[cfg(target_os = "linux")]
const OS: &str = "linux";

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

impl ToString for Error {
    fn to_string(&self) -> String {
        match self {
            Error::Reqwest(err) => err.to_string(),
            Error::Tokio(err) => err.to_string(),
            Error::SerdeJson(err) => err.to_string(),
        }
    }
}

impl AsyncLauncher {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    pub async fn get_version_manifest(&self) -> Result<types::VersionManifest, Error> {
        const VERSION_MANIFEST_URL: &str =
            "https://launchermeta.mojang.com/mc/game/version_manifest.json";

        self.client
            .get(VERSION_MANIFEST_URL)
            .send()
            .await
            .map_err(Error::Reqwest)?
            .json()
            .await
            .map_err(Error::Reqwest)
    }

    // Todo cache version json
    pub async fn get_version_json(
        &self,
        version_details: &types::Version,
        directory: &Path,
    ) -> Result<types::VersionJson, Error> {
        let directory = directory.join(&version_details.id);
        let file = directory.join(version_details.id.clone() + ".json");
        if tokio::fs::try_exists(&file).await.map_err(Error::Tokio)? {
            let buf = tokio::fs::read(file).await.map_err(Error::Tokio)?;
            serde_json::from_slice(&buf).map_err(Error::SerdeJson)
        } else {
            let buf = self
                .client
                .get(&version_details.url)
                .send()
                .await
                .map_err(Error::Reqwest)?
                .bytes()
                .await
                .map_err(Error::Reqwest)?;

            tokio::fs::create_dir_all(&directory)
                .await
                .map_err(Error::Tokio)?;

            tokio::fs::write(file, &buf).await.map_err(Error::Tokio)?;

            serde_json::from_slice(&buf).map_err(Error::SerdeJson)
        }
    }

    /// This expects a top level path, ie: "./Assets", and will append /indexes/ to the end to store them
    pub async fn get_asset_index_json(
        &self,
        asset_index: &types::AssetIndex,
        directory: &Path,
    ) -> Result<types::AssetIndexJson, Error> {
        let directory = directory.join("indexes");
        if tokio::fs::try_exists(&directory)
            .await
            .map_err(Error::Tokio)?
        {
            let buf = tokio::fs::read(directory).await.map_err(Error::Tokio)?;
            serde_json::from_slice(&buf).map_err(Error::SerdeJson)
        } else {
            let buf = self
                .client
                .get(&asset_index.url)
                .send()
                .await
                .map_err(Error::Reqwest)?
                .bytes()
                .await
                .map_err(Error::Reqwest)?;

            tokio::fs::create_dir(&directory)
                .await
                .map_err(Error::Tokio)?;

            tokio::fs::write(directory.join(&asset_index.id), &buf)
                .await
                .map_err(Error::Tokio)?;

            serde_json::from_slice(&buf).map_err(Error::SerdeJson)
        }
    }

    /// This expects a top level path, ie: "./Assets", and will append /objects/ to the end to store them
    pub async fn download_and_store_asset_index(
        &self,
        asset_index: &types::AssetIndexJson,
        directory: &Path,
    ) -> Result<(), Error> {
        const ASSET_BASE_URL: &str = "https://resources.download.minecraft.net";

        let object_path = directory.join("Objects");
        if !tokio::fs::try_exists(&object_path)
            .await
            .map_err(Error::Tokio)?
        {
            tokio::fs::create_dir(&object_path)
                .await
                .map_err(Error::Tokio)?;
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

                    if !dir_path.try_exists().map_err(Error::Tokio)? {
                        tokio::fs::create_dir(dir_path)
                            .await
                            .map_err(Error::Tokio)?;
                    }

                    let url = if file_path.try_exists().map_err(Error::Tokio)? {
                        let buf = tokio::fs::read(&file_path).await.map_err(Error::Tokio)?;
                        sha1.update(&buf);

                        let digest = sha1.digest().to_string();

                        if digest != asset.hash {
                            tokio::fs::remove_file(&file_path)
                                .await
                                .map_err(Error::Tokio)?;
                            format!("{}/{}/{}/", ASSET_BASE_URL, first_two, &asset.hash)
                        } else {
                            return Ok(());
                        }
                    } else {
                        format!("{}/{}/{}", ASSET_BASE_URL, first_two, &asset.hash)
                    };

                    let response = client.get(url).send().await.map_err(Error::Reqwest)?;
                    let bytes = response.bytes().await.map_err(Error::Reqwest)?;
                    tokio::fs::write(&file_path, bytes)
                        .await
                        .map_err(Error::Tokio)
                }
            })
            .await
    }

    pub async fn download_libraries_and_get_path(
        &self,
        libraries: &[types::Library],
        directory: &Path,
    ) -> Result<String, Error> {
        let mut path = String::new();

        stream::iter(libraries.iter().filter_map(|library| {
            path.reserve(library.name.len() + 1);
            let Some(library) = (if let Some(rules) = &library.rules {
                rules.iter().find_map(|rule| {
                    if let Some(os) = &rule.os {
                        if os.name == OS && rule.action == types::Action::Allow {
                            Some(library)
                        } else {
                            None
                        }
                    } else if rule.action == types::Action::Allow {
                        Some(library)
                    } else {
                        None
                    }
                })
            } else {
                Some(library)
            }) else {
                return None;
            };

            let artifact = if let Some(artifact) = &library.downloads.artifact {
                artifact
            } else if let Some(classifier) = &library.downloads.classifiers {
                let option = if cfg!(windows) {
                    classifier.natives_windows.as_ref()
                } else if cfg!(target_os = "macos") {
                    classifier.natives_osx.as_ref()
                } else if cfg!(target_os = "linux") {
                    if cfg!(target_arch = "x86_64") {
                        if let Some(native) = &classifier.natives_linux {
                            Some(native)
                        } else {
                            classifier.linux_x86_64.as_ref()
                        }
                    } else {
                        classifier.natives_linux.as_ref()
                    }
                } else {
                    unreachable!("Found bad metadata {:?} {:?}", classifier, library)
                };

                match option {
                    Some(art) => art,
                    None => return None,
                }
            } else {
                unreachable!("Found missing artifact")
            };

            path.extend([&library.name, ";"]);

            Some(Ok(artifact))
        }))
        .try_for_each_concurrent(16, |artifact| {
            let client = self.client.clone();
            let mut sha1 = sha1_smol::Sha1::new();
            let directory = &directory;
            async move {
                let path = directory.join(Path::new(&artifact.path));
                let parent = path.parent().unwrap();

                if tokio::fs::try_exists(&path).await.map_err(Error::Tokio)? {
                    let buf = tokio::fs::read(&path).await.map_err(Error::Tokio)?;
                    sha1.update(&buf);
                    if sha1.digest().to_string() == artifact.sha1 {
                        return Ok(());
                    } else {
                        tokio::fs::remove_file(&path).await.map_err(Error::Tokio)?;
                    }
                }

                let url = &artifact.url;
                let response = client.get(url).send().await.map_err(Error::Reqwest)?;
                let bytes = response.bytes().await.map_err(Error::Reqwest)?;
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(Error::Tokio)?;
                tokio::fs::write(path, bytes).await.map_err(Error::Tokio)
            }
        })
        .await?;

        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use reqwest::Client;

    use crate::{types::VersionJson, AsyncLauncher};

    #[tokio::test]
    async fn test_version_types() {
        let launcher = AsyncLauncher::new(Client::new());
        let manifest = launcher.get_version_manifest().await.unwrap();
        let _ = fs::create_dir("./Versions");
        for version in manifest.versions.iter() {
            if let Err(err) = launcher
                .get_version_json(version, &Path::new("./Versions"))
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
        let manifest = launcher.get_version_manifest().await.unwrap();
        fs::create_dir("./Assets").unwrap();
        if let Ok(VersionJson::Modern(version)) = launcher
            .get_version_json(&manifest.versions[0], &Path::new("./Versions"))
            .await
        {
            if let Ok(index) = launcher
                .get_asset_index_json(&version.asset_index, &Path::new("./Assets"))
                .await
            {
                if let Err(err) = launcher
                    .download_and_store_asset_index(&index, &Path::new("./Assets"))
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
        let manifest = launcher.get_version_manifest().await.unwrap();
        fs::create_dir("./Libs").unwrap();
        for version in &manifest.versions {
            let libs = match launcher
                .get_version_json(version, &Path::new("./Versions"))
                .await
            {
                Ok(VersionJson::Modern(version)) => version.libraries,
                Ok(VersionJson::Legacy(version)) => version.libraries,
                Ok(VersionJson::Ancient(version)) => version.libraries,
                Err(err) => panic!("How {err:?}"),
            };
            // println!("{:?}", version.id);
            launcher
                .download_libraries_and_get_path(&libs, &Path::new("./Libs"))
                .await
                .unwrap();
            // println!("{}", path);
            break;
        }
        fs::remove_dir_all("./Libs").unwrap();
    }

    #[tokio::test]
    async fn write_new_file() {
        let path = Path::new("./Help/Me.txt");
        let parent = path.parent().unwrap();
        tokio::fs::create_dir_all(parent).await.unwrap();
        tokio::fs::write("./Help/Me.txt", b"Help").await.unwrap();
    }
}
