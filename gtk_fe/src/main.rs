use gtk4 as gtk;
use gtk4::glib::property::PropertyGet;
use gtk4::glib::WeakRef;
use gtk4::prelude::{
    ApplicationExt, ApplicationExtManual, BoxExt, ButtonExt, GridExt, GtkWindowExt,
};
use gtk4::{
    Application, ApplicationWindow, Grid, Label, ListBox, Orientation, Overflow, PolicyType,
    ScrolledWindow,
};
use launcher_core::types::Version;
use std::path::Path;

fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RUNTIME.get_or_init(|| tokio::runtime::Runtime::new().unwrap())
}

fn launcher() -> &'static launcher_core::AsyncLauncher {
    static LAUNCHER: std::sync::OnceLock<launcher_core::AsyncLauncher> = std::sync::OnceLock::new();
    LAUNCHER.get_or_init(|| launcher_core::AsyncLauncher::new(reqwest::Client::new()))
}

fn main() -> gtk4::glib::ExitCode {
    let app = Application::builder()
        .application_id("com.also_sylv.synth_launcher")
        .build();

    let (sender, receiver) = async_channel::unbounded();

    let sender_ref = sender.clone();

    runtime().spawn(async move {
        let message = launcher().get_version_manifest(Path::new("./")).await;

        sender_ref.send(message).await.unwrap()
    });

    app.connect_activate(move |app| {
        let receiver_ref = receiver.clone();

        let mut vec: Vec<Version> = Vec::new();

        let combo_box = ListBox::builder().build();

        combo_box.set_placeholder(Some(&Label::new(Some("Loading..."))));

        combo_box.connect_row_selected(|list, row| {
            println!("Selected! {row:?}");
        });

        let weak = WeakRef::new();
        weak.set(Some(&combo_box));

        let button = gtk::Button::builder().label("Play").build();

        button.connect_clicked({
            move |_| {
                let row = weak.get(|list| list.as_ref().unwrap().selected_row());
                println!("{:?}", row)
            }
        });

        let grid = Grid::builder()
            .column_spacing(10)
            .row_spacing(10)
            .height_request(1)
            .width_request(1)
            .hexpand(false)
            .vexpand(false)
            .overflow(Overflow::Hidden)
            .column_homogeneous(false)
            .row_homogeneous(false)
            .build();
        grid.attach(&Label::new(Some("Cell One")), 0, 0, 1, 1);
        grid.attach(&Label::new(Some("Cell Two")), 1, 0, 1, 1);
        grid.attach(
            &Label::new(Some("REALLY LONG CELL NAME BECAUSE I HATE YOU")),
            0,
            1,
            1,
            1,
        );

        let hori_box = gtk4::Box::builder()
            .orientation(Orientation::Horizontal)
            .build();
        let vert_box = gtk4::Box::builder()
            .orientation(Orientation::Vertical)
            .build();

        let nhori_box = gtk::Box::builder()
            .orientation(Orientation::Horizontal)
            .build();

        let nvert_box = gtk4::Box::builder()
            .orientation(Orientation::Vertical)
            .build();

        nhori_box.append(&grid);
        nvert_box.append(&nhori_box);

        let scrolled_window = ScrolledWindow::builder()
            .hscrollbar_policy(PolicyType::Never)
            .width_request(100)
            .height_request(100)
            .child(&combo_box)
            .build();

        vert_box.append(&scrolled_window);
        vert_box.append(&button);
        hori_box.append(&vert_box);
        hori_box.append(&nvert_box);

        let window = ApplicationWindow::builder()
            .application(app)
            .default_height(500)
            .default_width(500)
            .child(&hori_box)
            .build();

        gtk4::glib::spawn_future_local({
            async move {
                while let Ok(response) = receiver_ref.recv().await {
                    if let Ok(list) = response {
                        vec.extend(list.versions);
                        for i in &vec {
                            let label = Label::new(Some(&i.id));
                            combo_box.append(&label);
                        }
                    }
                }
            }
        });

        window.present();
    });

    app.run()
}
