mod mpris_client;
mod progress_ring_button;
mod ui;

use gtk::prelude::*;
use libadwaita as adw;

const APP_ID: &str = "com.github.toasterrepair.empress";

fn main() {
    let app = adw::Application::builder().application_id(APP_ID).build();

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

        /* Album art placeholder - bold initial on shaded background */
        .album-art-placeholder {
            font-size: 4rem;
            font-weight: 800;
            color: alpha(@window_fg_color, 0.6);
            background-color: @shade_color;
            border-radius: 24px;
            box-shadow: 0 4px 12px rgba(0, 0, 0, 0.12),
                        0 8px 24px rgba(0, 0, 0, 0.06);
            min-width: 180px;
            min-height: 180px;
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

        /* Play/Pause button - use GNOME accent color */
        progressringbutton button.play-pause {
            background-color: @accent_bg_color;
            color: @accent_fg_color;
            transition: background-color 250ms ease-in-out,
                        color 250ms ease-in-out;
        }

        progressringbutton button.play-pause:hover {
            background-color: mix(@accent_bg_color, white, 0.1);
        }

        progressringbutton button.play-pause:active {
            background-color: mix(@accent_bg_color, black, 0.15);
        }

        /* Paused state - translucent background, accent foreground (icon + radial) */
        progressringbutton button.play-pause.paused {
            background-color: alpha(@accent_bg_color, 0.2);
            color: @accent_bg_color;
        }

        progressringbutton button.play-pause.paused:hover {
            background-color: alpha(@accent_bg_color, 0.3);
        }

        progressringbutton button.play-pause.paused:active {
            background-color: alpha(@accent_bg_color, 0.45);
        }

        /* Prev/Next buttons - slightly smaller for visual hierarchy */
        .circular.flat {
            min-width: 40px;
            min-height: 40px;
            transition: background-color 150ms ease-out;
        }

        .circular.flat:hover {
            background-color: alpha(@window_fg_color, 0.08);
        }

        .circular.flat:active {
            background-color: alpha(@window_fg_color, 0.15);
        }

        /* Volume slider - adopt the user's selected accent color.
           The .accent class alone doesn't reliably color the inner
           highlight node on all libadwaita versions, so target it
           explicitly. */
        scale.accent > trough > highlight {
            background-color: @accent_bg_color;
            border-radius: 999px;
            transition: background-color 150ms ease-out;
        }

        scale.accent > trough > slider {
            background-color: @accent_bg_color;
            transition: background-color 150ms ease-out,
                        box-shadow 150ms ease-out;
        }

        scale.accent > trough > slider:hover {
            background-color: mix(@accent_bg_color, white, 0.1);
            box-shadow: 0 0 0 4px alpha(@accent_bg_color, 0.15);
        }

        scale.accent > trough > slider:active {
            background-color: mix(@accent_bg_color, black, 0.15);
        }

        /* Give the trough itself a subtle hover glow */
        scale.accent trough:hover {
            background-color: @button_hover_bg_color;
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
