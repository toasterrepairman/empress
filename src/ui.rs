use gtk::prelude::*;
use gtk::glib;
use libadwaita as adw;
use adw::prelude::*;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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

    // Add drag gesture to move window on the album art area only
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
    content.art_container.add_controller(drag_gesture);

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

    // Track last known art URL, when we last tried to load it, and if it was successfully loaded (for HTTP URLs)
    let last_art_info = Arc::new(Mutex::new((None::<String>, Instant::now(), false)));

    // Clone for the first closure
    let last_art_info_for_updates = last_art_info.clone();

    // Poll the receiver from the main GTK thread
    glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
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
                // Check if art URL has changed to determine if we should force art update
                let force_art_update = if let Ok(last_info) = last_art_info_for_updates.lock() {
                    last_info.0.as_ref() != info.art_url.as_ref()
                } else {
                    true // If we can't check, assume we need to update
                };

                update_ui_widgets(&title_label, &artist_label, &album_label, &album_art, &art_container, &play_pause_button, &info, force_art_update);

                // Update last known art URL when it changes
                if let Some(ref art_url) = info.art_url {
                    if let Ok(mut last_info) = last_art_info_for_updates.lock() {
                        if last_info.0.as_ref() != Some(art_url) {
                            let is_http = art_url.starts_with("http://") || art_url.starts_with("https://");
                            *last_info = (Some(art_url.clone()), Instant::now(), !is_http); // Only set loaded=true for non-HTTP URLs initially
                        }
                    }
                }
            }
        }
        glib::ControlFlow::Continue
    });

    // Start improved periodic art check (every 3 seconds to reduce excessive retries)
    let album_art_clone = content.album_art.clone();
    let art_container_clone = content.art_container.clone();
    let last_art_info_clone = last_art_info.clone();

    glib::timeout_add_local(Duration::from_millis(3000), move || {
        let album_art = album_art_clone.clone();
        let art_container = art_container_clone.clone();
        let last_art_info = last_art_info_clone.clone();

        // Check if we should retry art loading
        let (should_retry, art_url_to_retry) = if let Ok(last_info) = last_art_info.lock() {
            let (ref last_art_url, last_attempt_time, already_loaded) = *last_info;

            // Retry conditions:
            // 1. We have an art URL
            // 2. Either container is not visible (failed load) OR art has no paintable (load issue) OR not yet loaded (for HTTP URLs)
            // 3. Enough time has passed since last attempt
            if let Some(ref art_url) = last_art_url {
                let is_http = art_url.starts_with("http://") || art_url.starts_with("https://");
                let should_retry = (!art_container.is_visible() || album_art.paintable().is_none() || (is_http && !already_loaded))
                    && last_attempt_time.elapsed() >= Duration::from_millis(3000);
                (should_retry, Some(art_url.clone()))
            } else {
                (false, None)
            }
        } else {
            (false, None)
        };

        if let Some(art_url) = art_url_to_retry {
            if should_retry {
                // Better URL handling: strip "file://" and handle URL encoding
                let file_path = if let Some(stripped) = art_url.strip_prefix("file://") {
                    stripped
                } else {
                    &art_url
                };

                // Handle different types of art URLs
                if art_url.starts_with("http://") || art_url.starts_with("https://") {
                    // For web URLs, download the image data first
                    match reqwest::blocking::get(art_url.as_str()) {
                        Ok(response) => {
                            match response.bytes() {
                                Ok(bytes) => {
                                    let bytes_vec = bytes.to_vec();
                                    // Create a memory input stream from the bytes
                                    let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from(&bytes_vec));
                                    // Use GdkPixbuf's from_stream method which can handle various image formats
                                    match gdk_pixbuf::Pixbuf::from_stream(&stream, gio::Cancellable::NONE) {
                                        Ok(pixbuf) => {
                                            let texture = gdk::Texture::for_pixbuf(&pixbuf);
                                            album_art.set_paintable(Some(&texture));
                                            art_container.set_visible(true);
                                            eprintln!("Successfully loaded art from web: {}", art_url);

                                            // Mark as loaded to prevent further retries
                                            if let Ok(mut last_info) = last_art_info.lock() {
                                                *last_info = (Some(art_url.clone()), Instant::now(), true);
                                            }
                                        }
                                        Err(e) => {
                                            eprintln!("Retry failed to create pixbuf from web data {}: {}", art_url, e);
                                            // Update the last attempt time
                                            if let Ok(mut last_info) = last_art_info.lock() {
                                                *last_info = (Some(art_url), Instant::now(), false);
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Retry failed to read bytes from web {}: {}", art_url, e);
                                    // Update the last attempt time
                                    if let Ok(mut last_info) = last_art_info.lock() {
                                        *last_info = (Some(art_url), Instant::now(), false);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Retry failed to download image from web {}: {}", art_url, e);
                            // Update the last attempt time
                            if let Ok(mut last_info) = last_art_info.lock() {
                                *last_info = (Some(art_url), Instant::now(), false);
                            }
                        }
                    }
                } else {
                    // For file:// or local paths, decode and load from filesystem
                    // Handle URL encoding for special characters
                    let decoded_path = urlencoding::decode(file_path).unwrap_or_else(|_| file_path.into());
                    let decoded_path_str = decoded_path.as_ref();

                    // Try to load the art file with better error handling
                    match std::path::Path::new(decoded_path_str).exists() {
                        true => {
                            match gdk_pixbuf::Pixbuf::from_file(decoded_path_str) {
                                Ok(pixbuf) => {
                                    let texture = gdk::Texture::for_pixbuf(&pixbuf);
                                    album_art.set_paintable(Some(&texture));
                                    art_container.set_visible(true);

                                    // Clear the last art URL since we succeeded
                                    if let Ok(mut last_info) = last_art_info.lock() {
                                        *last_info = (None, Instant::now(), false);
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Retry failed to load pixbuf from {}: {}", decoded_path, e);
                                    // Update the last attempt time
                                    if let Ok(mut last_info) = last_art_info.lock() {
                                        *last_info = (Some(art_url), Instant::now(), false);
                                    }
                                }
                            }
                        }
                        false => {
                            eprintln!("Retry: Art file still does not exist: {}", decoded_path);
                            // Update the last attempt time even if file doesn't exist
                            if let Ok(mut last_info) = last_art_info.lock() {
                                *last_info = (Some(art_url), Instant::now(), false);
                            }
                        }
                    }
                }
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
    force_art_update: bool,
) {
    title_label.set_text(&info.title);
    artist_label.set_text(&info.artist);
    album_label.set_text(&info.album);

    artist_label.set_visible(!info.artist.is_empty());
    album_label.set_visible(!info.album.is_empty());

    // Handle album art loading with better error handling - only update when forced
    if force_art_update {

        if let Some(ref art_url) = info.art_url {
            // Better URL handling: strip "file://" and handle URL encoding
            let file_path = if let Some(stripped) = art_url.strip_prefix("file://") {
                stripped
            } else {
                art_url
            };

            // Handle different types of art URLs
            if art_url.starts_with("http://") || art_url.starts_with("https://") {
                // For web URLs, download the image data first
                match reqwest::blocking::get(art_url.as_str()) {
                    Ok(response) => {
                        match response.bytes() {
                            Ok(bytes) => {
                                let bytes_vec = bytes.to_vec();
                                // Create a memory input stream from the bytes
                                let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from(&bytes_vec));
                                // Use GdkPixbuf's from_stream method which can handle various image formats
                                match gdk_pixbuf::Pixbuf::from_stream(&stream, gio::Cancellable::NONE) {
                                    Ok(pixbuf) => {
                                        let texture = gdk::Texture::for_pixbuf(&pixbuf);
                                        album_art.set_paintable(Some(&texture));
                                        art_container.set_visible(true);
                                        // Only log on initial load, not on retry mechanism
                                        // Retry mechanism will handle logging
                                    }
                                    Err(e) => {
                                        eprintln!("Failed to create pixbuf from web data {}: {}", art_url, e);
                                        album_art.set_paintable(gtk::gdk::Paintable::NONE);
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to read bytes from web {}: {}", art_url, e);
                                album_art.set_paintable(gtk::gdk::Paintable::NONE);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to download image from web {}: {}", art_url, e);
                        album_art.set_paintable(gtk::gdk::Paintable::NONE);
                    }
                }
            } else {
                // For file:// or local paths, decode and load from filesystem
                // Handle URL encoding for special characters
                let decoded_path = urlencoding::decode(file_path).unwrap_or_else(|_| file_path.into());
                let decoded_path_str = decoded_path.as_ref();

                // Try to load the art file
                match std::path::Path::new(decoded_path_str).exists() {
                    true => {
                        match gdk_pixbuf::Pixbuf::from_file(decoded_path_str) {
                            Ok(pixbuf) => {
                                let texture = gdk::Texture::for_pixbuf(&pixbuf);
                                album_art.set_paintable(Some(&texture));
                                art_container.set_visible(true);
                                eprintln!("Successfully loaded art from file: {}", decoded_path);
                            }
                            Err(e) => {
                                eprintln!("Failed to load pixbuf from {}: {}", decoded_path, e);
                                // Don't hide container immediately - let retry mechanism handle it
                                album_art.set_paintable(gtk::gdk::Paintable::NONE);
                            }
                        }
                    }
                    false => {
                        eprintln!("Art file does not exist: {}", decoded_path);
                        album_art.set_paintable(gtk::gdk::Paintable::NONE);
                    }
                }
            }
        } else {
            // No art URL provided, clear art and hide container
            album_art.set_paintable(gtk::gdk::Paintable::NONE);
            art_container.set_visible(false);
        }
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
