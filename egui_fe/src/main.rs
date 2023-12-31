mod worker_logic;
mod wrappers;

use std::fs::File;
use std::io::{Read, Write};
use worker_logic::*;
use wrappers::*;

use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use eframe::egui::{self, Button, Label, Sense};
use launcher_core::account::types::Account;
use launcher_core::{
    types::{AssetIndexJson, VersionJson, VersionManifest},
    AsyncLauncher,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};

// TODO: Store encrypted auth token for reuse: Use Keyring crate
// TODO: Document existing UI functionality: In-Progress
// TODO: Make vector of accounts, allow account selection
// TODO: Make instances, must be savable to disk, maybe using RON?: Json Format
// TODO: Redo error handling, fields that can error should hold Result<T, E>
// UPDATE: We could also add a tag to the error? Not sure. Constant Error checking would suck.
struct LauncherGui {
    // Async thread pool to handle futures
    rt: async_bridge::Runtime<Message, Response, State>,
    // receiver for messages sent before the event is finished
    rx: async_channel::Receiver<EarlyMessage>,
    // Reference to the async launcher
    launcher: Arc<AsyncLauncher>,
    // Minecraft Data
    data: MCData,
    // Data related to the player
    player: PlayerData,
    launcher_path: Arc<PathBuf>,
    // Current major java version
    java_version: u32,
    jvm_index: Option<usize>,
    current_error: Option<Error>,
    // Path to JVM, if changed
    // Flipped once for startup tasks
    config: Config,
}

#[derive(Default)]
struct PlayerData {
    // Player account, if it exists
    account: Option<Account>,
    // URL for auth, if it exists
    url: Option<String>,
    // Code will always exist if URL does
    code: Option<String>,
}

#[derive(Default)]
struct MCData {
    // Version Manifest read/write able
    versions: Option<VersionManifest>,
    // Holds the index in manifest for the current selected version
    selected_version: usize,
    // Version JSON, read only
    version_json: Option<Arc<VersionJson>>,
    // Asset Index, read only
    asset_index: Option<Arc<AssetIndexJson>>,
    // Classpath for the MC jar, also doubles as verifying
    // That libraries are completely loaded before launch
    class_path: Option<String>,
    // The jar path, stored separate because futures are not ordered
    // This is None if the version has changed
    jar_path: Option<String>,
    // Total and finished libraries, divide as floats
    // and multiply by 100 to get progress as percentage
    total_libraries: Arc<AtomicUsize>,
    finished_libraries: Arc<AtomicUsize>,
    // Total and finished assets, divide as floats
    // and multiply by 100 to get progress as percentage
    total_assets: Arc<AtomicUsize>,
    finished_assets: Arc<AtomicUsize>,
    // Total progress downloading the MC jar
    total_jar: Arc<AtomicUsize>,
    finished_jar: Arc<AtomicUsize>,
    // Whether or not all assets are loaded
    assets: bool,
    // If the launcher is attempting to launch
    launching: bool,
}

impl LauncherGui {
    fn new(cc: &eframe::CreationContext) -> Box<Self> {
        let (config_dir, config) = check_config().unwrap();

        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(4)
            .build()
            .expect("Runtime Failed to Build");

        let client = Client::new();
        let launcher_core = Arc::new(AsyncLauncher::new(client.clone()));
        let (tx, rx) = async_channel::unbounded();

        let rt = async_bridge::Runtime::new(
            4,
            State {
                client,
                launcher_core: launcher_core.clone(),
                tx,
            },
            cc.egui_ctx.clone(),
            worker_event_loop,
            rt,
        );

        let launcher_path = Arc::new(config_dir);

        rt.future(get_default_version_response());
        send_message(&rt, Contents::Auth, &launcher_path);
        send_message(&rt, Contents::Versions, &launcher_path);

        LauncherGui {
            rt,
            rx,
            launcher: launcher_core.clone(),
            player: Default::default(),
            data: MCData {
                // Defaults to usize::MAX to show no version is selected
                selected_version: usize::MAX,
                ..Default::default()
            },
            launcher_path,
            java_version: u32::MAX,
            current_error: None,
            jvm_index: None,
            config
        }.into()
    }

    fn update_state(&mut self, _: &egui::Context) {
        let event = self.rt.try_recv();
        if let Ok(message) = event {
            match message {
                Response::Versions(version) => match version {
                    Ok(versions) => self.data.versions = Some(versions),
                    Err(err) => {
                        dbg!("{}", &err);
                        self.current_error = Some(err.into())
                    },
                },
                Response::Version(version) => match version {
                    Ok(json) => self.data.version_json = Some(json.into()),
                    Err(err) => {
                        dbg!("{}", &err);
                        self.current_error = Some(err.into())
                    },
                },
                Response::Auth(res) => match res {
                    Ok(acc) => self.player.account = Some(acc),
                    Err(err) => {
                        dbg!("{}", &err);
                        self.current_error = Some(err.into())
                    },
                },
                Response::JavaMajorVersion(version) | Response::DefaultJavaVersion(version) => {
                    self.java_version = version.unwrap();
                }
                Response::Tagged(response, tag) => {
                    if let Some(versions) = &self.data.versions {
                        match response {
                            TaggedResponse::Libraries(result) => match result {
                                Ok(path) => {
                                    if versions.versions[self.data.selected_version] == tag {
                                        self.data.class_path = Some(path);
                                    }
                                }
                                Err(err) => self.current_error = Some(err.into()),
                            },
                            TaggedResponse::AssetIndex(idx) => match idx {
                                Ok(json) => {
                                    if versions.versions[self.data.selected_version] == tag {
                                        let index = Arc::new(json);

                                        let future = get_assets(
                                            self.launcher.clone(),
                                            index.clone(),
                                            self.launcher_path.clone(),
                                            self.data.total_assets.clone(),
                                            self.data.finished_assets.clone(),
                                            tag.clone(),
                                        );

                                        self.rt.future(future);

                                        self.data.asset_index = Some(index);
                                    }
                                }
                                Err(err) => self.current_error = Some(err.into()),
                            },
                            TaggedResponse::Asset(result) => match result {
                                Ok(()) => {
                                    if versions.versions[self.data.selected_version] == tag {
                                        self.data.assets = true;
                                    }
                                }
                                Err(err) => self.current_error = Some(err.into()),
                            },
                            TaggedResponse::Jar(res) => match res {
                                Ok(jar) => {
                                    if versions.versions[self.data.selected_version] == tag {
                                        self.data.jar_path = Some(jar);
                                    }
                                }
                                Err(err) => self.current_error = Some(err.into()),
                            },
                        }
                    }
                }
            }
        }

        if let Ok(val) = self.rx.try_recv() {
            match val {
                EarlyMessage::LinkCode((url, code)) => {
                    self.player.code = Some(code);
                    self.player.url = Some(url);
                }
            }
        }
    }

    fn prepare_launch(&self, json: &Arc<VersionJson>, manifest: &VersionManifest) {
        let libraries = json.libraries().clone().into();
        let index = json.asset_index().clone();
        let current = self.data.selected_version;
        let tag = manifest.versions[current].clone();

        let future = get_asset_index(
            self.launcher.clone(),
            index,
            tag.clone(),
            self.launcher_path.clone(),
        );
        self.rt.future(future);
        let future = get_libraries(
            self.launcher.clone(),
            libraries,
            self.launcher_path.clone(),
            self.data.total_libraries.clone(),
            self.data.finished_libraries.clone(),
            tag.clone(),
        );
        self.rt.future(future);
        let future = get_jar(
            self.launcher.clone(),
            json.clone(),
            self.launcher_path.clone(),
            self.data.total_jar.clone(),
            self.data.finished_jar.clone(),
            tag.clone(),
        );
        self.rt.future(future);
    }

    fn maybe_launch(&self) -> bool {
        if let (Some(class_path), Some(json), Some(acc), Some(jar_path)) = (
            &self.data.class_path,
            &self.data.version_json,
            &self.player.account,
            &self.data.jar_path,
        ) {
            if self.data.assets && self.data.launching {
                let jvm = if let Some(jvm) = self.jvm_index {
                    &self.config.jvms[jvm].path
                } else {
                    "java"
                };

                launcher_core::launch_game(
                    jvm,
                    json,
                    &self.launcher_path,
                    &self.launcher_path.join("assets"),
                    acc,
                    CLIENT_ID,
                    "0",
                    "Synth Launcher",
                    "0.1.0",
                    &format!("{}{}", class_path, jar_path),
                );
                !self.data.launching
            } else {
                self.data.launching
            }
        } else {
            self.data.launching
        }
    }

    fn progress_window(&self, ctx: &egui::Context) {
        egui::Window::new("Progress").auto_sized().show(ctx, |ui| {
            let percentage =
                |finished: usize, total: usize| -> f64 { (finished as f64 / total as f64) * 100.0 };

            let maybe_total = self.data.total_libraries.load(Ordering::Relaxed);
            let finished = self.data.finished_libraries.load(Ordering::Relaxed);

            // Ensure we're not dividing by 0
            let total = if maybe_total == 0 { 1 } else { maybe_total };
            let string = format!("Library Progress: {:.2} %", percentage(finished, total));
            ui.label(string);

            let maybe_total = self.data.total_assets.load(Ordering::Relaxed);
            let finished = self.data.finished_assets.load(Ordering::Relaxed);

            // Ensure we're not dividing by 0
            let total = if maybe_total == 0 { 1 } else { maybe_total };
            let string = format!("Asset Progress: {:.2} %", percentage(finished, total));
            ui.label(string);

            if self.data.jar_path.is_none() {
                let maybe_total = self.data.total_jar.load(Ordering::Relaxed);
                let finished = self.data.finished_jar.load(Ordering::Relaxed);

                // Ensure we're not dividing by 0
                let total = if maybe_total == 0 { 1 } else { maybe_total };
                let string = format!("Jar Progress: {:.2} %", percentage(finished, total));
                ui.label(string);
            } else {
                ui.label("Jar Progress: 100.00%");
            }

            ctx.request_repaint();
        });
    }
}

fn send_message<R, M>(rt: &async_bridge::Runtime<Message, R, M>, contents: Contents, launcher_path: &Arc<PathBuf>) where R: Send, M: Clone + Send + Sync {
    rt.send_with_message(Message {
        path: launcher_path.clone(),
        contents,
    });
}

impl eframe::App for LauncherGui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update_state(ctx);

        let mut config_updated = false;

        if let Some(error) = &self.current_error {
            egui::Window::new("Help").auto_sized().show(ctx, |ui| {
                ui.label(error.to_string());
            });
        }

        if self.player.account.is_none() {
            egui::Window::new("Login").auto_sized().show(ctx, |ui| {
                if let (Some(url), Some(code)) = (&self.player.url, &self.player.code) {
                    let hyper = egui::Hyperlink::from_label_and_url("Click here to login", url);
                    let label = Label::new(code).sense(Sense::click());
                    let label = ui.add(label).on_hover_ui(|ui| {
                        ui.label("Copy this token into the site below!");
                    });

                    if label.clicked() {
                        ctx.copy_text(code.to_string());
                    }
                    ui.add(hyper);
                } else {
                    ui.label("Loading code and url, please wait...");
                }
            });
        }

        let size = ctx.input(|i| i.screen_rect());
        let width = size.width();
        let height = size.height();

        egui::SidePanel::left("General Panel")
            .exact_width(width * 0.1)
            .resizable(false)
            .show(ctx, |ui| {
                if let Some(versions) = &self.data.versions {
                    let index = self.data.selected_version;
                    let text = if index != usize::MAX {
                        &versions.versions[index].id
                    } else {
                        "None"
                    };

                    ui.add_space(height * 0.01);

                    egui::ComboBox::from_id_source("VersionSelect")
                        .selected_text(text)
                        .show_ui(ui, |ui| {
                            let iter = versions.versions.iter().enumerate();
                            iter.for_each(|(index, version)| {
                                let selected = &mut self.data.selected_version;

                                if ui.selectable_value(selected, index, &version.id).clicked() {
                                    let launcher = self.launcher.clone();
                                    let version = version.clone();
                                    let path = self.launcher_path.clone();

                                    if let Some(json) = &self.data.version_json {
                                        if version.id != json.id() {
                                            self.data.version_json = None;
                                            self.data.class_path = None;
                                            self.data.jar_path = None;
                                            self.data.assets = false;
                                            self.rt.future(get_version(launcher, version, path));
                                        }
                                    } else {
                                        self.rt.future(get_version(launcher, version, path));
                                    }
                                };
                            })
                        });

                    let selected_text = if let Some(jvm_index) = self.jvm_index {
                        &self.config.jvms[jvm_index].name
                    } else {
                        "Default"
                    };

                    egui::ComboBox::from_id_source("Java Selector").wrap(true).selected_text(selected_text).show_ui(ui, |ui| {
                        if ui.button("Default").clicked() {
                            self.jvm_index = None;
                            self.rt.future(get_default_version_response());
                        }

                        for (index, jvm) in self.config.jvms.iter().enumerate() {
                            if ui.button(jvm.name.as_str()).clicked() {
                                self.jvm_index = Some(index);
                                let (_vendor, version) = get_vendor_major_version(&jvm.path);
                                self.java_version = version;
                            }
                        }
                    });

                    if self.java_version != u32::MAX {
                        ui.label(format!("Java Version: {}", self.java_version));
                    } else {
                        ui.label("No Java Version");
                    };


                    if ui.button("Add Java Version").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_file() {
                            let path = path.display().to_string();
                            let (vendor, version) = get_vendor_major_version(&path);
                            self.config.jvms.push(Jvm { path, name: format!("{vendor} {version}") });
                            config_updated = true;
                        }
                    }

                    let button = Button::new("Play");

                    if let Some(version_json) = &self.data.version_json {
                        let enabled = !self.data.launching && self.player.account.is_some();
                        let enabled = ui.add_enabled(enabled, button);

                        if enabled.clicked() {
                            self.prepare_launch(version_json, versions);
                            self.data.launching = true;
                        }
                    } else {
                        ui.add_enabled(false, button);
                    }
                } else {
                    ui.spinner();
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            // Why does this panel get created?
        });

        if self.data.launching {
            self.progress_window(ctx);
        }

        self.data.launching = self.maybe_launch();

        if config_updated {
            let bytes = toml::to_string_pretty(&self.config).unwrap();
            std::fs::write(self.launcher_path.join("config.toml"), bytes.as_bytes()).unwrap();
        }
    }
}

fn check_config() -> Result<(PathBuf, Config), Error> {
    let app_dir = platform_dirs::AppDirs::new(Some("synth_launcher"), false).unwrap();

    let config: Config;
    let config_file_loc = app_dir.config_dir.join("config.toml");

    if !app_dir.config_dir.try_exists()? {
        std::fs::create_dir(&app_dir.config_dir)?;
    }

    if config_file_loc.exists() {
        let mut file = File::open(&config_file_loc)?;
        let mut buffer = String::new();
        file.read_to_string(&mut buffer)?;
        config = toml::from_str(&buffer)?;
    } else {
        config = Config::default();
        let mut file = File::create(&config_file_loc)?;
        let string = toml::to_string(&config)?;
        file.write_all(string.as_bytes())?
    }

    Ok((app_dir.config_dir, config))
}

#[derive(Default, Deserialize, Serialize)]
struct Config {
    jvms: Vec<Jvm>,
    accounts: Vec<Account>
}

#[derive(Default, Deserialize, Serialize)]
struct Jvm {
    path: String,
    name: String,
}

fn main() {

    eframe::run_native(
        "Test App",
        eframe::NativeOptions::default(),
        Box::new(|cc| LauncherGui::new(cc)),
    )
    .unwrap();
}

#[derive(Debug)]
enum Error {
    Reqwest(reqwest::Error),
    Tokio(tokio::io::Error),
    SerdeJson(serde_json::Error),
    TomlDE(toml::de::Error),
    TomlSER(toml::ser::Error)
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

impl From<toml::de::Error> for Error {
    fn from(value: toml::de::Error) -> Self {
        Error::TomlDE(value)
    }
}

impl From<toml::ser::Error> for Error {
    fn from(value: toml::ser::Error) -> Self {
        Error::TomlSER(value)
    }
}


impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            Error::Reqwest(err) => err.to_string(),
            Error::Tokio(err) => err.to_string(),
            Error::SerdeJson(err) => err.to_string(),
            Error::TomlDE(err) => err.to_string(),
            Error::TomlSER(err) => err.to_string(),
        };
        write!(f, "{}", str)
    }
}

impl From<launcher_core::Error> for Error {
    fn from(value: launcher_core::Error) -> Self {
        match value {
            launcher_core::Error::Reqwest(e) => {
                Error::Reqwest(e)
            }
            launcher_core::Error::Tokio(e) => {
                Error::Tokio(e)
            }
            launcher_core::Error::SerdeJson(e) => {
                Error::SerdeJson(e)
            }
        }
    }
}