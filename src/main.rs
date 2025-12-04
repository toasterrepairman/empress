mod mpris_client;
mod ui;
mod progress_ring_button;

use gtk::prelude::*;
use libadwaita as adw;

const APP_ID: &str = "com.github.toasterrepair.empress";

fn main() {
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_activate(|app| {
        // Load custom CSS
        load_css();

        let window = ui::build_ui(app);
        window.present();
    });

    app.run();
}

fn load_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(
        r#"

        "#,
    );

    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("Could not connect to a display."),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}
