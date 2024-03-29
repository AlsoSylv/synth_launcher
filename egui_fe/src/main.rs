mod instances;
mod worker_logic;
mod wrappers;

use std::cell::Cell;
use std::fs::File;
use std::io::Write;
use worker_logic::*;
use wrappers::*;

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::AtomicU64;
use std::sync::{atomic::Ordering, Arc};
use std::time::SystemTime;

use eframe::egui::panel::TopBottomSide::Bottom;
use eframe::egui::style::Spacing;
use eframe::egui::{
    self, Align, Button, Color32, FontId, Frame, Image, Label, Layout, Margin, Pos2, Rect, Sense,
    Stroke, Ui, Vec2, Vec2b,
};
use eframe::emath::RectTransform;
use launcher_core::account::types::Account;
use launcher_core::types::{Latest, Type, Version};
use launcher_core::{
    types::{AssetIndexJson, VersionJson, VersionManifest},
    AsyncLauncher,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use instances::*;

// TODO: Store encrypted auth token for reuse: Use Keyring crate
// TODO: Document existing UI functionality: In-Progress
// TODO: Redo error handling, fields that can error should hold Result<T, E>
// UPDATE: We could also add a tag to the error? Not sure. Constant Error checking would suck.
struct LauncherGui {
    // Async thread pool to handle futures
    rt: async_bridge::Runtime<Message, Response, State>,
    // receiver for messages sent before the event is finished
    rx: async_channel::Receiver<(String, String)>,
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
    launcher_data: LauncherData,
    // Holds the position of the dots in the loading message
    loading_place: SystemTime,
    data_updated: bool,
    adding_account: bool,
    adding_instance: bool,
    temp_instance: InstanceBuilder,
    instances: Vec<EguiInstance>,
    current_instance: Option<usize>,
    quick_playing: bool,
}

#[derive(Default)]
struct PlayerData {
    // Player account, if it exists
    account: Option<usize>,
    // URL for auth, if it exists
    url: Option<String>,
    // Code will always exist if URL does
    code: Option<String>,
}

#[derive(Default)]
struct MCData {
    // Version Manifest read/write able
    versions: Option<VersionManifestArc>,
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
    total_libraries: Arc<AtomicU64>,
    finished_libraries: Arc<AtomicU64>,
    // Total and finished assets, divide as floats
    // and multiply by 100 to get progress as percentage
    total_assets: Arc<AtomicU64>,
    finished_assets: Arc<AtomicU64>,
    // Total progress downloading the MC jar
    total_jar: Arc<AtomicU64>,
    finished_jar: Arc<AtomicU64>,
    // Whether all assets are loaded
    assets: bool,
    // If the launcher is attempting to launch
    launching: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VersionManifestArc {
    pub latest: Latest,
    pub versions: Vec<Arc<Version>>,
}

impl From<VersionManifest> for VersionManifestArc {
    fn from(mut value: VersionManifest) -> Self {
        let mut arc_versions = Vec::with_capacity(value.versions.len());

        for version in value.versions.drain(..) {
            arc_versions.push(Arc::new(version))
        }

        Self {
            latest: value.latest,
            versions: arc_versions,
        }
    }
}

impl VersionManifestArc {
    pub fn latest_release(&self) -> &Arc<Version> {
        for version in &self.versions {
            if version.id == self.latest.release {
                return version;
            }
        }

        // If the latest release does not exist in the meta, things have probably gone wrong lol
        unreachable!()
    }
}

#[derive(Default, Deserialize, Serialize)]
struct LauncherData {
    jvms: Vec<Rc<Jvm>>,
    accounts: Vec<AccRefreshPair>,
    instances: Vec<Rc<Instance>>,
}

#[derive(Deserialize, Serialize)]
struct AccRefreshPair {
    account: Account,
    refresh_token: Arc<str>,
}

struct EguiInstance {
    i_instance: Rc<Instance>,
    image: Option<Image<'static>>,
    version_json: Cell<Option<Arc<VersionJson>>>,
    launching: Cell<bool>,
    prepared: Cell<bool>,
}

#[derive(Default)]
struct TempInstance {
    name: String,
    image: Option<PathBuf>,
    jvm: Option<Rc<Jvm>>,
    version: Option<Arc<Version>>,
    path: String,
    mod_loader: Option<Loader>,
    jvm_args: String,
    env_args: String,
}

impl From<TempInstance> for Instance {
    fn from(value: TempInstance) -> Self {
        Self {
            name: value.name,
            image: value.image,
            jvm: value.jvm.unwrap(),
            version: value.version.unwrap(),
            path: PathBuf::from(value.path),
            mod_loader: value.mod_loader,
            jvm_args: value.jvm_args.split(' ').map(String::from).collect(),
            env_args: value.env_args.split(' ').map(String::from).collect(),
        }
    }
}

impl LauncherGui {
    fn new(cc: &eframe::CreationContext) -> Box<Self> {
        let (config_dir, config) = check_file().unwrap();

        let egui_instances = config
            .instances
            .iter()
            .map(|instance| EguiInstance {
                i_instance: instance.clone(),
                image: instance
                    .image
                    .as_ref()
                    .map(|image| Image::from_uri(format!("file://{}", image.to_string_lossy()))),
                version_json: Cell::new(None),
                launching: false.into(),
                prepared: false.into(),
            })
            .collect();

        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(4)
            .build()
            .expect("Runtime Failed to Build");

        let client = Client::new();
        let launcher_core = Arc::new(AsyncLauncher::new(client.clone()));
        let (tx, rx) = async_channel::unbounded();

        let state = &*Box::leak(Box::new(State {
            client,
            launcher_core: launcher_core.clone(),
            tx,
        }));

        let rt = async_bridge::Runtime::new(4, state, cc.egui_ctx.clone(), worker_event_loop, rt);

        let launcher_path = Arc::new(config_dir);

        let (_, default_java_version) = get_vendor_major_version("java");

        send_message(&rt, Contents::Versions, &launcher_path);

        for acc in &config.accounts {
            send_message(
                &rt,
                Contents::Auth(Some(acc.refresh_token.clone())),
                &launcher_path,
            );
        }

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
            java_version: default_java_version,
            current_error: None,
            jvm_index: None,
            launcher_data: config,
            loading_place: SystemTime::now(),
            data_updated: false,
            adding_account: false,
            adding_instance: false,
            temp_instance: InstanceBuilder::default(),
            instances: egui_instances,
            current_instance: None,
            quick_playing: false,
        }
        .into()
    }

    fn current_tag<'a>(&'a self, versions: &'a VersionManifestArc) -> &'a Arc<Version> {
        if let Some(instance) = &self.current_instance {
            &self.instances[*instance].i_instance.version
        } else {
            &versions.versions[self.data.selected_version]
        }
    }

    fn update_state(&mut self, _: &egui::Context) -> Result<(), Error> {
        let event = self.rt.try_recv();
        if let Ok(message) = event {
            match message {
                Response::Versions(manifest) => self.data.versions = Some(manifest?.into()),
                Response::Version(json) => {
                    let arc: Arc<VersionJson> = json?.into();
                    for instances in &mut self.instances {
                        if instances.i_instance.version.id == arc.id {
                            instances.version_json.set(Some(arc.clone()));
                        }
                    }
                    self.data.version_json = Some(arc.clone())
                }
                Response::Auth(res) => {
                    let (acc, refresh) = res?;
                    let into = AccRefreshPair {
                        account: acc,
                        refresh_token: refresh.into(),
                    };
                    for acc in &mut self.launcher_data.accounts {
                        if acc.account.profile.id == into.account.profile.id {
                            *acc = into;
                            self.data_updated = true;
                            return Ok(());
                        }
                    }
                    self.launcher_data.accounts.push(into);
                    self.adding_account = false;
                    self.data_updated = true;
                }
                Response::Tagged(response, tag) => {
                    if let Some(versions) = &self.data.versions {
                        match response {
                            TaggedResponse::Libraries(result) => {
                                let path = result?;
                                if self.current_tag(versions) == &tag {
                                    self.data.class_path = Some(path);
                                }
                            }
                            TaggedResponse::AssetIndex(res) => {
                                let json = res?;
                                if self.current_tag(versions) == &tag {
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
                            TaggedResponse::Asset(result) => {
                                result?;
                                if self.current_tag(versions) == &tag {
                                    self.data.assets = true;
                                }
                            }
                            TaggedResponse::Jar(res) => {
                                let jar = res?;
                                if self.current_tag(versions) == &tag {
                                    self.data.jar_path = Some(jar);
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Ok((url, code)) = self.rx.try_recv() {
            self.player.code = Some(code);
            self.player.url = Some(url);
        }

        Ok(())
    }

    fn prepare_launch(&self, json: &Arc<VersionJson>, manifest: &VersionManifestArc) {
        let libraries = json.libraries().clone();
        let index = json.asset_index().clone();
        let tag = self.current_tag(manifest);

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

    fn maybe_launch(&self, json: &Arc<VersionJson>, jvm: Option<&Jvm>, current: bool) -> bool {
        if let (Some(class_path), Some(acc), Some(jar_path)) = (
            &self.data.class_path,
            self.player.account,
            &self.data.jar_path,
        ) {
            if self.data.assets && self.data.launching {
                let jvm = if let Some(jvm) = jvm {
                    jvm.path.as_str()
                } else if let Some(jvm) = self.jvm_index {
                    &self.launcher_data.jvms[jvm].path
                } else {
                    "java"
                };

                launcher_core::launch_game(
                    jvm,
                    json,
                    &self.launcher_path,
                    &self.launcher_path.join("assets"),
                    &self.launcher_data.accounts[acc].account,
                    CLIENT_ID,
                    "0",
                    "Synth Launcher",
                    "0.1.0",
                    &format!("{}{}", class_path, jar_path),
                );
                !current
            } else {
                current
            }
        } else {
            current
        }
    }

    fn progress_window(&self, ctx: &egui::Context) {
        egui::Window::new("Progress").auto_sized().show(ctx, |ui| {
            let percentage = |finished, total| (finished as f64 / total as f64) * 100.0;

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

    fn account_picker(&mut self, ui: &mut Ui) {
        let frame = Frame::canvas(ui.style())
            .inner_margin(Margin::ZERO)
            .stroke(Stroke::NONE);

        egui::TopBottomPanel::new(Bottom, "Bottom Panel")
            .frame(frame)
            .show_separator_line(false)
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    let button = Button::new("➕").small();

                    if ui.add_enabled(!self.adding_account, button).clicked() {
                        self.rt.send_with_message(Message {
                            path: self.launcher_path.clone(),
                            contents: Contents::Auth(None),
                        });
                        self.adding_account = true;
                    }

                    if let Some(acc_idx) = &mut self.player.account {
                        let name = &self.launcher_data.accounts[*acc_idx].account.profile.name;

                        egui::ComboBox::from_id_source("Account Picker")
                            .width(ui.available_width() * 0.80)
                            .selected_text(name)
                            .show_index(ui, acc_idx, self.launcher_data.accounts.len(), |idx| {
                                &self.launcher_data.accounts[idx].account.profile.name
                            });
                    } else if self.launcher_data.accounts.is_empty() {
                        ui.label("No Accounts");
                    } else {
                        self.player.account = Some(0)
                    };

                    let button = Button::new("➖").small();

                    if ui.add_enabled(!self.adding_account, button).clicked() {
                        todo!()
                    }
                });
            });
    }
}

fn send_message<R, M>(
    rt: &async_bridge::Runtime<Message, R, M>,
    contents: Contents,
    launcher_path: &Arc<PathBuf>,
) where
    R: Send,
    M: Clone + Send + Sync,
{
    rt.send_with_message(Message {
        path: launcher_path.clone(),
        contents,
    });
}

impl eframe::App for LauncherGui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Err(e) = self.update_state(ctx) {
            dbg!("{e}");
            self.current_error = Some(e);
        }

        if let Some(error) = &self.current_error {
            egui::Window::new("Help").auto_sized().show(ctx, |ui| {
                ui.label(error.to_string());
            });
        }

        if self.adding_account {
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
        // let height = size.height();

        ctx.style_mut(|style| {
            style.spacing.indent = 0.0;
        });

        egui::SidePanel::left("General Panel")
            .exact_width(width * 0.20)
            .resizable(false)
            .show(ctx, |ui| {
                if let Some(versions) = self.data.versions.take() {
                    self.account_picker(ui);

                    let index = &mut self.data.selected_version;
                    let text = if *index != usize::MAX {
                        &versions.versions[*index].id
                    } else {
                        "None"
                    };
                    let mut changed = false;

                    egui::ComboBox::from_id_source("VersionSelect")
                        .width(ui.available_width())
                        .selected_text(text)
                        .show_ui(ui, |ui| {
                            versions
                                .versions
                                .iter()
                                .enumerate()
                                .filter(|(_, v)| v.version_type == Type::Release)
                                .for_each(|(idx, val)| {
                                    if ui.selectable_value(index, idx, &val.id).clicked() {
                                        changed = true;
                                    }
                                });
                        });

                    if changed {
                        let version = &versions.versions[*index];
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
                    }

                    let selected_text = if let Some(jvm_index) = self.jvm_index {
                        &self.launcher_data.jvms[jvm_index].name
                    } else {
                        "Default"
                    };

                    egui::ComboBox::from_id_source("Java Selector")
                        .wrap(true)
                        .selected_text(selected_text)
                        .show_ui(ui, |ui| {
                            if ui.button("Default").clicked() {
                                self.jvm_index = None;
                                let (_vendor, version) = get_vendor_major_version("java");
                                self.java_version = version;
                            }

                            for (index, jvm) in self.launcher_data.jvms.iter().enumerate() {
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
                            self.launcher_data.jvms.push(Rc::new(Jvm {
                                path,
                                name: format!("{vendor} {version}"),
                            }));
                            self.data_updated = true;
                        }
                    }

                    let button = Button::new("Play");

                    if let Some(version_json) = &self.data.version_json {
                        let enabled = !self.data.launching && self.player.account.is_some();
                        let enabled = ui.add_enabled(enabled, button);

                        if enabled.clicked() {
                            self.prepare_launch(version_json, &versions);
                            self.data.launching = true;
                            self.quick_playing = true;
                        }
                    } else {
                        ui.add_enabled(false, button);
                    }

                    let button = Button::new("Add Instance");

                    if ui.add_enabled(!self.adding_instance, button).clicked() {
                        self.adding_instance = true;
                        self.temp_instance = Default::default();
                    }

                    self.data.versions = Some(versions);
                } else {
                    let mut loading = "Loading".to_string();
                    let elapsed = self.loading_place.elapsed().unwrap();
                    for _ in 0..elapsed.as_secs() {
                        loading.push('.');
                    }
                    if elapsed.as_secs() > 3 {
                        self.loading_place = SystemTime::now()
                    };

                    ui.label(loading);
                    ctx.request_repaint();
                }
            });

        if self.adding_instance {
            egui::Window::new("Adding Instance").show(ctx, |ui| {
                let tmp = &mut self.temp_instance;

                ui.horizontal(|ui| {
                    ui.label("Name: ");
                    ui.text_edit_singleline(tmp.name_mut());
                });

                ui.horizontal(|ui| {
                    ui.label("JVM: ");

                    let selected_text = tmp.jvm().name.as_str();

                    egui::ComboBox::from_id_source("Java Selector")
                        .wrap(true)
                        .selected_text(selected_text)
                        .show_ui(ui, |ui| {
                            if ui.button("Default").clicked() {
                                tmp.jvm = Default::default();
                            }

                            for jvm in &self.launcher_data.jvms {
                                if ui.button(jvm.name.as_str()).clicked() {
                                    tmp.jvm = jvm.clone();
                                }
                            }
                        });
                });

                ui.horizontal(|ui| {
                    let label = Label::new("Select Icon Path").sense(Sense::click());
                    if ui.add(label).clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_file() {
                            tmp.image = Some(path.to_string_lossy().to_string());
                        }
                    }
                });

                ui.horizontal(|ui| {
                    if let Some(versions) = &self.data.versions {
                        let selected_text = if let Some(v) = tmp.version() {
                            v.id.as_str()
                        } else {
                            "None"
                        };

                        egui::ComboBox::from_id_source("VersionSelect")
                            .selected_text(selected_text)
                            .show_ui(ui, |ui| {
                                let iter = versions.versions.iter();
                                iter.for_each(|version| {
                                    if ui.button(&version.id).clicked() {
                                        tmp.version = Some(version.clone());
                                    };
                                })
                            });
                    }
                });

                ui.horizontal(|ui| {
                    if ui.button("Select Path").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            tmp.path = path.to_string_lossy().to_string();
                        }
                    }

                    ui.text_edit_singleline(tmp.path_mut());
                });

                ui.horizontal(|ui| {
                    ui.label("Jvm Args: ");
                    ui.text_edit_singleline(tmp.jvm_args_mut());
                });

                ui.horizontal(|ui| {
                    ui.label("Env Args: ");
                    ui.text_edit_singleline(tmp.env_args_mut());
                });

                ui.horizontal(|ui| {
                    ui.radio_value(tmp.mod_loader_mut(), None, "Vanilla");
                    ui.radio_value(tmp.mod_loader_mut(), Some(Loader::Fabric), "Fabric");
                });

                if ui.button("Add").clicked() {
                    let tmp = std::mem::take(tmp);
                    let instance: Rc<Instance> = Rc::new(tmp.build());

                    self.launcher_data.instances.push(instance.clone());

                    let image = instance.image.as_ref().map(|image_path| {
                        Image::from_uri(format!("file://{}", image_path.to_string_lossy()))
                    });

                    let egui_i = EguiInstance {
                        i_instance: instance.clone(),
                        image,
                        version_json: Cell::new(None),
                        launching: false.into(),
                        prepared: false.into(),
                    };

                    self.instances.push(egui_i);
                    self.adding_instance = false;
                    self.data_updated = true;
                }
            });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::new(Vec2b { x: false, y: true }).show(ui, |ui| {
                let (response, _painter) = ui.allocate_painter(
                    Vec2::new(ui.available_width(), ui.available_height()),
                    Sense::hover(),
                );
                let to_screen = RectTransform::from_to(
                    Rect::from_min_size(Pos2::ZERO, response.rect.size()),
                    response.rect,
                );

                let mut max_idx = 0;
                let mut row = 0;

                let len = ui.fonts(|fonts| fonts.glyph_width(&FontId::default(), 'W') * 10.0);

                ui.style_mut().spacing = Spacing::default();

                for (idx, instances) in self.instances.iter().enumerate() {
                    if ui.available_width() - len * (idx - max_idx) as f32 <= len {
                        row += 1;
                        max_idx = idx;
                    }

                    let mut clicked = false;

                    ui.put(
                        Rect {
                            min: to_screen.transform_pos(Pos2 {
                                x: 10.0 + len * (idx - max_idx) as f32,
                                y: 0.0 + (row * 100) as f32,
                            }),
                            max: to_screen.transform_pos(Pos2 {
                                x: 150.0 + len * (idx - max_idx) as f32,
                                y: 100.0 + (row * 100) as f32,
                            }),
                        },
                        |ui: &mut Ui| {
                            ui.horizontal(|ui| {
                                ui.add_space(10.0);
                                ui.vertical(|ui| {
                                    ui.style_mut().visuals.window_fill = Color32::WHITE;

                                    if let Some(image) = &instances.image {
                                        ui.add(image.clone());
                                    }
                                    let label =
                                        Label::new(&instances.i_instance.name).truncate(true);
                                    ui.add(label);
                                    ui.label(&instances.i_instance.version.id);
                                    ui.label(&instances.i_instance.jvm.name);

                                    let button = Button::new("Play");

                                    if let Some(manifest) = &self.data.versions {
                                        let enabled =
                                            !self.data.launching && self.player.account.is_some();

                                        let res = ui.add_enabled(enabled, button);

                                        if res.clicked() {
                                            let launcher = self.launcher.clone();
                                            let version = instances.i_instance.version.clone();
                                            let path = self.launcher_path.clone();
                                            self.rt.future(get_version(launcher, version, path));
                                            instances.launching.replace(true);
                                            instances.prepared.replace(false);
                                            clicked = true
                                        }

                                        if let Some(json) = instances.version_json.take() {
                                            if instances.launching.get()
                                                && !instances.prepared.get()
                                            {
                                                self.prepare_launch(&json, manifest);
                                                instances.prepared.replace(true);
                                            } else {
                                                let maybe_launched = self.maybe_launch(
                                                    &json,
                                                    Some(&instances.i_instance.jvm),
                                                    true,
                                                );

                                                instances.launching.replace(maybe_launched);
                                            }

                                            instances.version_json.set(Some(json));
                                        }
                                    } else {
                                        ui.add_enabled(false, button);
                                    }
                                });

                                ui.with_layout(Layout::right_to_left(Align::BOTTOM), |ui| {
                                    ui.separator();
                                });
                            })
                            .response
                        },
                    );

                    if clicked {
                        self.current_instance = Some(idx);
                        self.data.launching = true;
                    }
                }
            });
        });

        if self.data.launching {
            if let Some(json) = &self.data.version_json {
                if self.quick_playing {
                    self.data.launching = self.maybe_launch(json, None, self.data.launching);
                    self.quick_playing = self.data.launching;
                }
            }
            self.progress_window(ctx);
        }

        if self.data_updated {
            let bytes = toml::to_string_pretty(&self.launcher_data).unwrap();
            std::fs::write(
                self.launcher_path.join("launcher_data.toml"),
                bytes.as_bytes(),
            )
            .unwrap();
            self.data_updated = false;
        }
    }
}

fn check_file() -> Result<(PathBuf, LauncherData), Error> {
    let app_dir = platform_dirs::AppDirs::new(Some("synth_launcher"), false).unwrap();

    let launcher_data: LauncherData;
    let launcher_data_file = app_dir.config_dir.join("launcher_data.toml");

    if !app_dir.config_dir.try_exists()? {
        std::fs::create_dir(&app_dir.config_dir)?;
    }

    if launcher_data_file.exists() {
        let buffer = std::fs::read_to_string(&launcher_data_file)?;
        launcher_data = toml::from_str(&buffer)?;
    } else {
        launcher_data = LauncherData::default();
        let mut file = File::create(&launcher_data_file)?;
        let string = toml::to_string(&launcher_data)?;
        file.write_all(string.as_bytes())?
    }

    Ok((app_dir.config_dir, launcher_data))
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
    TomlSER(toml::ser::Error),
    Profile(launcher_core::account::types::ProfileError),
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
        let str: &dyn std::fmt::Display = match self {
            Error::Reqwest(err) => err,
            Error::Tokio(err) => err,
            Error::SerdeJson(err) => err,
            Error::TomlDE(err) => err,
            Error::TomlSER(err) => err,
            Error::Profile(err) => err,
        };
        write!(f, "{}", str)
    }
}

impl From<launcher_core::Error> for Error {
    fn from(value: launcher_core::Error) -> Self {
        match value {
            launcher_core::Error::Reqwest(e) => Error::Reqwest(e),
            launcher_core::Error::Tokio(e) => Error::Tokio(e),
            launcher_core::Error::SerdeJson(e) => Error::SerdeJson(e),
            launcher_core::Error::ProfileError(e) => Error::Profile(e),
        }
    }
}
