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
        /* Album art with Libadwaita-style rounded corners and elegant shadow */
        .album-art {
            border-radius: 24px;
            box-shadow: 0 4px 12px rgba(0, 0, 0, 0.12),
                        0 8px 24px rgba(0, 0, 0, 0.06);
            background-color: @shade_color;
        }

        /* Title styling - using Libadwaita heading styles */
        .title-1 {
            font-size: 1.5rem;
            font-weight: 700;
            letter-spacing: -0.02em;
        }

        /* Artist styling - subtitle style */
        .title-3 {
            font-size: 1rem;
            font-weight: 500;
        }

        /* Album caption - dimmed for visual hierarchy */
        .caption {
            font-size: 0.875rem;
        }

        /* Custom progress ring button styling - larger for better touch targets */
        progressringbutton {
            padding: 6px;
        }

        progressringbutton button {
            min-width: 52px;
            min-height: 52px;
        }

        /* Prev/Next buttons - slightly smaller for visual hierarchy */
        .circular.flat {
            min-width: 40px;
            min-height: 40px;
        }

        /* Ensure proper dark mode support */
        .album-art {
            background-color: @shade_color;
        }

        /* Subtle hover effects for buttons */
        .circular {
            transition: all 150ms ease-out;
        }

        .circular:hover {
            background-color: @hover_bg_color;
        }

        /* Smooth transitions for interactive elements */
        button, picture {
            transition: opacity 150ms ease-out;
        }

        /* Dropdown styling */
        dropdown {
            min-width: 150px;
        }

        /* Compact header bar */
        headerbar {
            min-height: 42px;
        }
        "#,
    );

    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("Could not connect to a display."),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}
