mod mpris_client;
mod ui;

use gtk::prelude::*;
use libadwaita as adw;

const APP_ID: &str = "com.github.empress";

fn main() {
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_activate(|app| {
        let window = ui::build_ui(app);
        window.present();
    });

    app.run();
}
