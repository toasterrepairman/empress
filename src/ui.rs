use gtk::prelude::*;
use gtk::glib;
use libadwaita as adw;
use adw::prelude::*;

use crate::mpris_client::{MprisClient, MediaInfo, PlayerStatus};
use crate::progress_ring_button::ProgressRingButton;

pub fn build_ui(app: &adw::Application) -> adw::ApplicationWindow {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Empress")
        .default_width(400)
        .default_height(500)
        .build();

    window.set_size_request(200, 200);

    let header_bar = adw::HeaderBar::new();

    let main_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header_bar);

    let content = build_content();

    // Add drag gesture to move window on the content area
    let drag_gesture = gtk::GestureDrag::new();
    drag_gesture.connect_drag_begin({
        let window = window.clone();
        move |gesture, _x, _y| {
            if let Some(device) = gesture.device() {
                if let Some(surface) = window.surface() {
                    if let Ok(toplevel) = surface.downcast::<gtk::gdk::Toplevel>() {
                        toplevel.begin_move(&device, 1, 0.0, 0.0, gtk::gdk::CURRENT_TIME);
                    }
                }
            }
        }
    });
    content.container.add_controller(drag_gesture);

    toolbar_view.set_content(Some(&content.container));

    main_box.append(&toolbar_view);
    window.set_content(Some(&main_box));

    let mpris_client = MprisClient::new();
    let media_receiver = mpris_client.start_monitoring();

    let title_label = content.title_label.downgrade();
    let artist_label = content.artist_label.downgrade();
    let album_label = content.album_label.downgrade();
    let album_art = content.album_art.downgrade();
    let art_container = content.art_container.downgrade();
    let play_pause_button = content.play_pause_button.downgrade();

    // Poll the receiver from the main GTK thread
    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        // Process all available messages
        while let Ok(info) = media_receiver.try_recv() {
            let title_label = title_label.upgrade();
            let artist_label = artist_label.upgrade();
            let album_label = album_label.upgrade();
            let album_art = album_art.upgrade();
            let art_container = art_container.upgrade();
            let play_pause_button = play_pause_button.upgrade();

            if let (Some(title_label), Some(artist_label), Some(album_label), Some(album_art), Some(art_container), Some(play_pause_button)) =
                (title_label, artist_label, album_label, album_art, art_container, play_pause_button)
            {
                update_ui_widgets(&title_label, &artist_label, &album_label, &album_art, &art_container, &play_pause_button, &info);
            }
        }
        glib::ControlFlow::Continue
    });

    setup_controls(&content, mpris_client.clone());
    setup_keyboard_shortcuts(&window, mpris_client);

    // Set play/pause button as the default focus
    let play_pause_button = content.play_pause_button.clone();
    window.connect_show(move |_| {
        play_pause_button.grab_focus();
    });

    window
}

#[derive(Clone)]
struct MediaContent {
    container: gtk::Box,
    album_art: gtk::Picture,
    art_container: gtk::Box,
    title_label: gtk::Label,
    artist_label: gtk::Label,
    album_label: gtk::Label,
    play_pause_button: ProgressRingButton,
    prev_button: gtk::Button,
    next_button: gtk::Button,
}

fn build_content() -> MediaContent {
    let container = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .valign(gtk::Align::Fill)
        .halign(gtk::Align::Fill)
        .vexpand(true)
        .hexpand(true)
        .build();

    let art_container = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .vexpand(true)
        .hexpand(true)
        .build();

    let album_art = gtk::Picture::builder()
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .can_shrink(true)
        .keep_aspect_ratio(true)
        .content_fit(gtk::ContentFit::Cover)
        .vexpand(false)
        .hexpand(false)
        .css_classes(vec!["album-art"])
        .build();

    art_container.append(&album_art);

    let info_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .halign(gtk::Align::Fill)
        .hexpand(true)
        .build();

    let title_label = gtk::Label::builder()
        .label("No media playing")
        .css_classes(vec!["title-1"])
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .justify(gtk::Justification::Center)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .lines(2)
        .build();

    let artist_label = gtk::Label::builder()
        .label("")
        .css_classes(vec!["title-3"])
        .wrap(true)
        .justify(gtk::Justification::Center)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .lines(1)
        .opacity(0.7)
        .build();

    let album_label = gtk::Label::builder()
        .label("")
        .css_classes(vec!["caption"])
        .wrap(true)
        .justify(gtk::Justification::Center)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .lines(1)
        .opacity(0.55)
        .build();

    info_box.append(&title_label);
    info_box.append(&artist_label);
    info_box.append(&album_label);

    let controls_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .halign(gtk::Align::Center)
        .margin_top(8)
        .build();

    let prev_button = gtk::Button::builder()
        .icon_name("media-skip-backward-symbolic")
        .css_classes(vec!["circular", "flat"])
        .build();

    let play_pause_button = ProgressRingButton::new();

    let next_button = gtk::Button::builder()
        .icon_name("media-skip-forward-symbolic")
        .css_classes(vec!["circular", "flat"])
        .build();

    controls_box.append(&prev_button);
    controls_box.append(&play_pause_button);
    controls_box.append(&next_button);

    container.append(&art_container);
    container.append(&info_box);
    container.append(&controls_box);

    art_container.set_visible(false);

    MediaContent {
        container,
        album_art,
        art_container,
        title_label,
        artist_label,
        album_label,
        play_pause_button,
        prev_button,
        next_button,
    }
}

fn update_ui_widgets(
    title_label: &gtk::Label,
    artist_label: &gtk::Label,
    album_label: &gtk::Label,
    album_art: &gtk::Picture,
    art_container: &gtk::Box,
    play_pause_button: &ProgressRingButton,
    info: &MediaInfo,
) {
    title_label.set_text(&info.title);
    artist_label.set_text(&info.artist);
    album_label.set_text(&info.album);

    artist_label.set_visible(!info.artist.is_empty());
    album_label.set_visible(!info.album.is_empty());

    // Always update art container visibility based on current art availability
    if let Some(ref art_url) = info.art_url {
        let url = art_url.strip_prefix("file://").unwrap_or(art_url);

        match gdk_pixbuf::Pixbuf::from_file(url) {
            Ok(pixbuf) => {
                let texture = gdk::Texture::for_pixbuf(&pixbuf);
                album_art.set_paintable(Some(&texture));
                art_container.set_visible(true);
            }
            Err(_) => {
                // Failed to load art, hide container
                album_art.set_paintable(gtk::gdk::Paintable::NONE);
                art_container.set_visible(false);
            }
        }
    } else {
        // No art URL provided, hide container
        album_art.set_paintable(gtk::gdk::Paintable::NONE);
        art_container.set_visible(false);
    }

    let is_paused = match info.status {
        PlayerStatus::Playing => false,
        _ => true,
    };
    let icon_name = if is_paused {
        "media-playback-start-symbolic"
    } else {
        "media-playback-pause-symbolic"
    };
    play_pause_button.set_icon_name(icon_name);
    play_pause_button.set_paused_style(is_paused);

    // Update progress ring
    if let (Some(position), Some(length)) = (info.position, info.length) {
        let progress = if length.as_secs() > 0 {
            position.as_secs_f64() / length.as_secs_f64()
        } else {
            0.0
        };
        play_pause_button.set_progress(progress);
    } else {
        play_pause_button.set_progress(0.0);
    }
}

fn setup_controls(content: &MediaContent, client: MprisClient) {
    content.play_pause_button.button().connect_clicked({
        let client = client.clone();
        move |_| {
            let _ = client.play_pause();
        }
    });

    content.next_button.connect_clicked({
        let client = client.clone();
        move |_| {
            let _ = client.next();
        }
    });

    content.prev_button.connect_clicked({
        let client = client.clone();
        move |_| {
            let _ = client.previous();
        }
    });

    // Add scroll event handler for seeking
    let scroll_controller = gtk::EventControllerScroll::new(
        gtk::EventControllerScrollFlags::VERTICAL,
    );

    scroll_controller.connect_scroll({
        let client = client.clone();
        move |_, _dx, dy| {
            // dy > 0 means scrolling down (go back 5 seconds)
            // dy < 0 means scrolling up (go forward 5 seconds)
            let offset_seconds = if dy > 0.0 {
                -5
            } else {
                5
            };

            // MPRIS seek uses microseconds
            let offset_micros = offset_seconds * 1_000_000;
            let _ = client.seek(offset_micros);

            glib::Propagation::Stop
        }
    });

    content.play_pause_button.add_controller(scroll_controller);
}

fn setup_keyboard_shortcuts(window: &adw::ApplicationWindow, client: MprisClient) {
    let event_controller = gtk::EventControllerKey::new();

    event_controller.connect_key_pressed({
        let client = client.clone();
        let window = window.clone();
        move |_, key, _code, modifier| {
            // Ctrl+Q to quit
            if key == gtk::gdk::Key::q && modifier == gtk::gdk::ModifierType::CONTROL_MASK {
                window.close();
                return glib::Propagation::Stop;
            }

            // Up arrow to play
            if key == gtk::gdk::Key::Up && modifier.is_empty() {
                let _ = client.play_pause();
                return glib::Propagation::Stop;
            }

            // Down arrow to pause
            if key == gtk::gdk::Key::Down && modifier.is_empty() {
                let _ = client.play_pause();
                return glib::Propagation::Stop;
            }

            // Left arrow for previous
            if key == gtk::gdk::Key::Left && modifier.is_empty() {
                let _ = client.previous();
                return glib::Propagation::Stop;
            }

            // Right arrow for next
            if key == gtk::gdk::Key::Right && modifier.is_empty() {
                let _ = client.next();
                return glib::Propagation::Stop;
            }

            glib::Propagation::Proceed
        }
    });

    window.add_controller(event_controller);
}
