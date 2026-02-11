use adw::prelude::*;
use gtk::glib;
use gtk::prelude::*;
use gtk::StringObject;
use libadwaita as adw;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::mpris_client::{MediaInfo, MprisClient, PlayerStatus};
use crate::progress_ring_button::ProgressRingButton;

#[derive(Clone)]
struct StatusHistoryEntry {
    status: PlayerStatus,
    title: String,
    artist: String,
    timestamp: Instant,
}

#[derive(Clone)]
struct SidebarContent {
    container: gtk::Box,
    list_box: gtk::ListBox,
}

pub fn build_ui(app: &adw::Application) -> adw::ApplicationWindow {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Empress")
        .default_width(320)
        .default_height(400)
        .build();

    window.set_size_request(150, 150);

    let header_bar = adw::HeaderBar::new();
    header_bar.set_show_title(false);

    // Create combo box for player selection
    let player_list = gtk::StringList::new(&[]);
    let player_combo = gtk::DropDown::builder()
        .model(&player_list)
        .tooltip_text("Select MPRIS player")
        .build();

    // Add "Auto" option as default
    player_list.append("Auto");

    // Pack the combo box into the header bar
    header_bar.pack_end(&player_combo);

    let sidebar = build_sidebar();
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

    let main_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header_bar);

    // Create a horizontal paned for main content and sidebar
    let paned = gtk::Paned::builder()
        .orientation(gtk::Orientation::Horizontal)
        .shrink_start_child(false)
        .shrink_end_child(false)
        .build();

    paned.set_start_child(Some(&sidebar.container));
    paned.set_end_child(Some(&content.clamp));

    // Initially hide sidebar
    sidebar.container.set_visible(false);

    toolbar_view.set_content(Some(&paned));
    main_box.append(&toolbar_view);
    window.set_content(Some(&main_box));

    let mpris_client = MprisClient::new();
    let media_receiver = mpris_client.start_monitoring();

    // Set up player combo box functionality
    let player_list_clone = player_list.clone();
    let player_combo_clone = player_combo.clone();
    let mpris_client_for_combo = mpris_client.clone();

    // Refresh player list every 5 seconds
    glib::timeout_add_local(Duration::from_secs(5), move || {
        // Get current selection
        let current_selected = player_combo_clone.selected();

        // Clear and repopulate (keeping "Auto" at index 0)
        while player_list_clone.n_items() > 1 {
            player_list_clone.remove(1);
        }

        let available = MprisClient::get_available_players();
        for player in &available {
            player_list_clone.append(player);
        }

        // Restore selection if possible
        if current_selected < player_list_clone.n_items() {
            let _ = player_combo_clone.set_selected(current_selected);
        }

        glib::ControlFlow::Continue
    });

    // Initial population
    {
        let available = MprisClient::get_available_players();
        for player in &available {
            player_list.append(player);
        }
    }

    // Handle player selection changes
    player_combo.connect_selected_item_notify({
        let mpris_client = mpris_client_for_combo.clone();
        move |combo| {
            let selected = combo.selected();
            if selected == 0 {
                // "Auto" selected - clear preferred player
                mpris_client.set_preferred_player(None);
            } else {
                // Specific player selected
                if let Some(item) = combo.selected_item() {
                    if let Some(str_obj) = item.downcast_ref::<StringObject>() {
                        let player_name = str_obj.string().to_string();
                        mpris_client.set_preferred_player(Some(player_name));
                    }
                }
            }
        }
    });

    let title_label = content.title_label.downgrade();
    let artist_label = content.artist_label.downgrade();
    let album_label = content.album_label.downgrade();
    let album_art = content.album_art.downgrade();
    let art_container = content.art_container.downgrade();
    let play_pause_button = content.play_pause_button.downgrade();

    // Track last known art URL to detect changes
    let last_art_url = Arc::new(Mutex::new(None::<String>));
    let last_art_url_for_updates = last_art_url.clone();

    // Track status history
    let history: Arc<Mutex<VecDeque<StatusHistoryEntry>>> =
        Arc::new(Mutex::new(VecDeque::with_capacity(50)));
    let last_status = Arc::new(Mutex::new(PlayerStatus::Stopped));
    let last_title = Arc::new(Mutex::new(String::new()));
    let last_artist = Arc::new(Mutex::new(String::new()));

    // Track if initial load has been done
    let initial_load_done = Arc::new(Mutex::new(false));

    // Set up window resize handler to show/hide sidebar based on aspect ratio
    let window_clone = window.clone();
    let sidebar_clone = sidebar.clone();
    glib::timeout_add_local(Duration::from_millis(100), move || {
        let width = window_clone.width() as f64;
        let height = window_clone.height() as f64;
        let should_show = width > 1.3 * height;
        sidebar_clone.container.set_visible(should_show);
        glib::ControlFlow::Continue
    });

    // Sidebar references for updates
    let sidebar_list_box = sidebar.list_box.clone();
    let history_for_updates = history.clone();
    let last_status_for_updates = last_status.clone();
    let last_title_for_updates = last_title.clone();
    let last_artist_for_updates = last_artist.clone();

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

            if let (
                Some(title_label),
                Some(artist_label),
                Some(album_label),
                Some(album_art),
                Some(art_container),
                Some(play_pause_button),
            ) = (
                title_label,
                artist_label,
                album_label,
                album_art,
                art_container,
                play_pause_button,
            ) {
                // Check if we need to force art update (URL changed, title changed, or artist changed)
                let title_changed = if let Ok(last) = last_title_for_updates.lock() {
                    *last != info.title
                } else {
                    true
                };

                let artist_changed = if let Ok(last) = last_artist_for_updates.lock() {
                    *last != info.artist
                } else {
                    true
                };

                let url_changed = if let Ok(last_url) = last_art_url_for_updates.lock() {
                    last_url.as_ref() != info.art_url.as_ref()
                } else {
                    true
                };

                let is_initial = if let Ok(initial) = initial_load_done.lock() {
                    !*initial
                } else {
                    true
                };

                let force_art_update = is_initial || url_changed || title_changed || artist_changed;

                update_ui_widgets(
                    &title_label,
                    &artist_label,
                    &album_label,
                    &album_art,
                    &art_container,
                    &play_pause_button,
                    &info,
                    force_art_update,
                );

                // Update last known values when they change
                if url_changed || title_changed || artist_changed {
                    if let Some(ref art_url) = info.art_url {
                        if let Ok(mut last_url) = last_art_url_for_updates.lock() {
                            *last_url = Some(art_url.clone());
                        }
                    }
                }

                // Mark initial load as done
                if is_initial {
                    if let Ok(mut initial) = initial_load_done.lock() {
                        *initial = true;
                    }
                }

                // Check if status has changed and update history
                let status_changed = if let Ok(last) = last_status_for_updates.lock() {
                    *last != info.status
                } else {
                    true
                };

                if status_changed || title_changed || artist_changed {
                    // Update last known values
                    if let Ok(mut last) = last_status_for_updates.lock() {
                        *last = info.status.clone();
                    }
                    if let Ok(mut last) = last_title_for_updates.lock() {
                        *last = info.title.clone();
                    }
                    if let Ok(mut last) = last_artist_for_updates.lock() {
                        *last = info.artist.clone();
                    }

                    // Add to history
                    if let Ok(mut history) = history_for_updates.lock() {
                        let entry = StatusHistoryEntry {
                            status: info.status.clone(),
                            title: info.title.clone(),
                            artist: info.artist.clone(),
                            timestamp: Instant::now(),
                        };
                        history.push_front(entry);

                        // Limit history size
                        while history.len() > 50 {
                            history.pop_back();
                        }

                        // Update sidebar
                        update_sidebar(&sidebar_list_box, &history);
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
    clamp: adw::Clamp,
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
    // Main container using Clamp for content width following HIG
    let clamp = adw::Clamp::builder()
        .maximum_size(280)
        .tightening_threshold(200)
        .build();

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

    // Art container for album artwork with proper spacing
    let art_container = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .vexpand(false)
        .hexpand(false)
        .build();

    let album_art = gtk::Picture::builder()
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .can_shrink(true)
        .content_fit(gtk::ContentFit::Cover)
        .vexpand(true)
        .hexpand(true)
        .width_request(100)
        .height_request(100)
        .css_classes(vec!["album-art"])
        .build();

    art_container.append(&album_art);

    // Info section using proper Libadwaita patterns
    let info_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .halign(gtk::Align::Center)
        .margin_top(12)
        .build();

    let title_label = gtk::Label::builder()
        .label("No media playing")
        .css_classes(vec!["title-1"])
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .justify(gtk::Justification::Center)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .lines(2)
        .halign(gtk::Align::Center)
        .build();

    let artist_label = gtk::Label::builder()
        .label("")
        .css_classes(vec!["title-3"])
        .wrap(true)
        .justify(gtk::Justification::Center)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .lines(1)
        .opacity(0.7)
        .halign(gtk::Align::Center)
        .build();

    let album_label = gtk::Label::builder()
        .label("")
        .css_classes(vec!["caption"])
        .wrap(true)
        .justify(gtk::Justification::Center)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .lines(1)
        .opacity(0.55)
        .halign(gtk::Align::Center)
        .build();

    info_box.append(&title_label);
    info_box.append(&artist_label);
    info_box.append(&album_label);

    // Controls section with improved spacing and sizing
    let controls_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .halign(gtk::Align::Center)
        .margin_top(12)
        .margin_bottom(6)
        .build();

    let prev_button = gtk::Button::builder()
        .icon_name("media-skip-backward-symbolic")
        .css_classes(vec!["circular", "flat"])
        .tooltip_text("Previous")
        .build();

    let play_pause_button = ProgressRingButton::new();
    play_pause_button
        .button()
        .set_tooltip_text(Some("Play/Pause"));

    let next_button = gtk::Button::builder()
        .icon_name("media-skip-forward-symbolic")
        .css_classes(vec!["circular", "flat"])
        .tooltip_text("Next")
        .build();

    controls_box.append(&prev_button);
    controls_box.append(&play_pause_button);
    controls_box.append(&next_button);

    container.append(&art_container);
    container.append(&info_box);
    container.append(&controls_box);

    clamp.set_child(Some(&container));

    art_container.set_visible(false);

    MediaContent {
        container,
        clamp,
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
        // Clear existing art before loading new art
        if !info.art_url.is_some() || info.art_url.as_ref().map_or(true, |u| u.is_empty()) {
            album_art.set_paintable(gtk::gdk::Paintable::NONE);
            art_container.set_visible(false);
        } else if let Some(ref art_url) = info.art_url {
            // Better URL handling: strip "file://" and handle URL encoding
            let file_path = if let Some(stripped) = art_url.strip_prefix("file://") {
                stripped
            } else {
                art_url
            };

            // Handle different types of art URLs
            if art_url.starts_with("http://") || art_url.starts_with("https://") {
                // Clear art before attempting to load new art
                album_art.set_paintable(gtk::gdk::Paintable::NONE);

                // For web URLs, download the image data first
                match reqwest::blocking::get(art_url.as_str()) {
                    Ok(response) => {
                        match response.bytes() {
                            Ok(bytes) => {
                                let bytes_vec = bytes.to_vec();
                                // Create a memory input stream from the bytes
                                let stream = gio::MemoryInputStream::from_bytes(
                                    &glib::Bytes::from(&bytes_vec),
                                );
                                // Use GdkPixbuf's from_stream method which can handle various image formats
                                match gdk_pixbuf::Pixbuf::from_stream(
                                    &stream,
                                    gio::Cancellable::NONE,
                                ) {
                                    Ok(pixbuf) => {
                                        let texture = gdk::Texture::for_pixbuf(&pixbuf);
                                        album_art.set_paintable(Some(&texture));
                                        art_container.set_visible(true);
                                        album_art.queue_draw();
                                        // Only log on initial load, not on retry mechanism
                                        // Retry mechanism will handle logging
                                    }
                                    Err(e) => {
                                        eprintln!(
                                            "Failed to create pixbuf from web data {}: {}",
                                            art_url, e
                                        );
                                        album_art.set_paintable(gtk::gdk::Paintable::NONE);
                                        art_container.set_visible(false);
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to read bytes from web {}: {}", art_url, e);
                                album_art.set_paintable(gtk::gdk::Paintable::NONE);
                                art_container.set_visible(false);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to download image from web {}: {}", art_url, e);
                        album_art.set_paintable(gtk::gdk::Paintable::NONE);
                        art_container.set_visible(false);
                    }
                }
            } else {
                // For file:// or local paths, decode and load from filesystem
                // Handle URL encoding for special characters
                let decoded_path =
                    urlencoding::decode(file_path).unwrap_or_else(|_| file_path.into());
                let decoded_path_str = decoded_path.as_ref();

                // Clear art before attempting to load new art
                album_art.set_paintable(gtk::gdk::Paintable::NONE);

                // Try to load the art file
                match std::path::Path::new(decoded_path_str).exists() {
                    true => match gdk_pixbuf::Pixbuf::from_file(decoded_path_str) {
                        Ok(pixbuf) => {
                            let texture = gdk::Texture::for_pixbuf(&pixbuf);
                            album_art.set_paintable(Some(&texture));
                            art_container.set_visible(true);
                            album_art.queue_draw();
                            eprintln!("Successfully loaded art from file: {}", decoded_path);
                        }
                        Err(e) => {
                            eprintln!("Failed to load pixbuf from {}: {}", decoded_path, e);
                            album_art.set_paintable(gtk::gdk::Paintable::NONE);
                            art_container.set_visible(false);
                        }
                    },
                    false => {
                        eprintln!("Art file does not exist: {}", decoded_path);
                        album_art.set_paintable(gtk::gdk::Paintable::NONE);
                        art_container.set_visible(false);
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
    let scroll_controller =
        gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);

    scroll_controller.connect_scroll({
        let client = client.clone();
        move |_, _dx, dy| {
            // dy > 0 means scrolling down (go back 5 seconds)
            // dy < 0 means scrolling up (go forward 5 seconds)
            let offset_seconds = if dy > 0.0 { -5 } else { 5 };

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

fn build_sidebar() -> SidebarContent {
    let container = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .margin_start(6)
        .margin_end(6)
        .margin_top(6)
        .margin_bottom(6)
        .width_request(200)
        .build();

    let header_label = gtk::Label::builder()
        .label("Session History")
        .css_classes(vec!["heading"])
        .halign(gtk::Align::Start)
        .margin_bottom(6)
        .build();

    let list_box = gtk::ListBox::builder()
        .css_classes(vec!["boxed-list"])
        .vexpand(true)
        .selection_mode(gtk::SelectionMode::None)
        .build();

    let scrolled_window = gtk::ScrolledWindow::builder()
        .child(&list_box)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .propagate_natural_width(true)
        .build();

    container.append(&header_label);
    container.append(&scrolled_window);

    SidebarContent {
        container,
        list_box,
    }
}

fn update_sidebar(list_box: &gtk::ListBox, history: &VecDeque<StatusHistoryEntry>) {
    // Clear existing children
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    // Add history entries
    for entry in history {
        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .margin_start(8)
            .margin_end(8)
            .margin_top(6)
            .margin_bottom(6)
            .build();

        let status_icon = match entry.status {
            PlayerStatus::Playing => "media-playback-start-symbolic",
            PlayerStatus::Paused => "media-playback-pause-symbolic",
            PlayerStatus::Stopped => "media-playback-stop-symbolic",
        };

        let title_label = gtk::Label::builder()
            .label(&entry.title)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .halign(gtk::Align::Start)
            .css_classes(vec!["title-3"])
            .build();

        let artist_label = gtk::Label::builder()
            .label(&entry.artist)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .halign(gtk::Align::Start)
            .opacity(0.7)
            .build();

        let info_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();

        let icon = gtk::Image::builder()
            .icon_name(status_icon)
            .pixel_size(16)
            .build();

        info_row.append(&icon);
        info_row.append(&title_label);

        row.append(&info_row);
        if !entry.artist.is_empty() {
            row.append(&artist_label);
        }

        list_box.append(&row);
    }
}
