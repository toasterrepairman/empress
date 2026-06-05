use adw::prelude::*;
use gtk::glib;
use gtk::prelude::*;
use gtk::StringObject;
use libadwaita as adw;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
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

    window.set_icon_name(None);

    window.set_size_request(150, 150);

    let header_bar = adw::HeaderBar::new();
    header_bar.set_show_title(false);
    header_bar.set_title_widget(None::<&gtk::Widget>);

    // Create combo box for player selection
    let player_list = gtk::StringList::new(&[]);
    let player_combo = gtk::DropDown::builder()
        .model(&player_list)
        .tooltip_text("Select MPRIS player")
        .build();

    // Add "Auto" option as default
    player_list.append("Auto");

    let sidebar = build_sidebar();
    let content = build_content();

    player_combo.set_halign(gtk::Align::Center);
    player_combo.set_margin_top(6);
    player_combo.set_margin_bottom(6);
    content
        .content_column
        .insert_child_after(&player_combo, Some(&content.clamp));

    // Sidebar toggle button
    let sidebar_toggle = gtk::ToggleButton::builder()
        .icon_name("sidebar-show-symbolic")
        .tooltip_text("Toggle Sidebar")
        .active(false)
        .css_classes(vec!["flat"])
        .build();
    header_bar.pack_start(&sidebar_toggle);
    sidebar_toggle.connect_toggled({
        let sidebar_container = sidebar.container.clone();
        move |btn| {
            sidebar_container.set_visible(btn.is_active());
        }
    });

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
    paned.set_end_child(Some(&content.content_column));

    // Initially hide sidebar
    sidebar.container.set_visible(false);

    toolbar_view.set_content(Some(&paned));
    main_box.append(&toolbar_view);
    window.set_content(Some(&main_box));

    let mpris_client = MprisClient::new();
    let monitor_tick = mpris_client.take_monitor_tick().expect("monitor tick not taken");
    let media_receiver = mpris_client.start_monitoring(monitor_tick);

    // Set up player combo box functionality
    let player_list_clone = player_list.clone();
    let player_combo_clone = player_combo.clone();
    let mpris_client_for_combo = mpris_client.clone();

    // Flag to block the selection handler during combo refresh
    let is_refreshing = Arc::new(AtomicBool::new(false));
    let is_refreshing_for_refresh = is_refreshing.clone();
    let is_refreshing_for_handler = is_refreshing.clone();

    // Refresh player list every 5 seconds
    glib::timeout_add_local(Duration::from_secs(5), move || {
        // Block the selection handler while we repopulate the model
        is_refreshing_for_refresh.store(true, Ordering::SeqCst);

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

        is_refreshing_for_refresh.store(false, Ordering::SeqCst);

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
            // Skip during combo refresh to avoid resetting preferred player
            if is_refreshing_for_handler.load(Ordering::SeqCst) {
                return;
            }
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
    let placeholder_label = content.placeholder_label.downgrade();
    let art_container = content.art_container.downgrade();
    let play_pause_button = content.play_pause_button.downgrade();
    let volume_scale = content.volume_scale.downgrade();
    let volume_clamp = content.volume_clamp.downgrade();
    let volume_updating_for_updates = content.volume_updating.clone();

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
            let placeholder_label = placeholder_label.upgrade();
            let art_container = art_container.upgrade();
            let play_pause_button = play_pause_button.upgrade();
            let volume_scale = volume_scale.upgrade();
            let volume_clamp = volume_clamp.upgrade();

            if let (
                Some(title_label),
                Some(artist_label),
                Some(album_label),
                Some(album_art),
                Some(placeholder_label),
                Some(art_container),
                Some(play_pause_button),
                Some(volume_scale),
                Some(volume_clamp),
            ) = (
                title_label,
                artist_label,
                album_label,
                album_art,
                placeholder_label,
                art_container,
                play_pause_button,
                volume_scale,
                volume_clamp,
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
                    &placeholder_label,
                    &art_container,
                    &play_pause_button,
                    &info,
                    force_art_update,
                );

                // Volume slider: hide when not controllable; otherwise reflect
                // the player's volume. Don't fight the pointer during a drag.
                let controllable = info.can_control && info.volume.is_some();
                volume_clamp.set_visible(controllable);
                if controllable {
                    if let Some(v) = info.volume {
                        if !volume_updating_for_updates.load(Ordering::SeqCst) {
                            let clamped = v.max(0.0).min(1.0);
                            // set_value is a no-op (no value-changed emission)
                            // when the value is unchanged, so this can't
                            // bounce back to MPRIS.
                            volume_scale.set_value(clamped);
                        }
                    }
                }

                // Update last known values when they change
                if url_changed || title_changed || artist_changed {
                    if let Ok(mut last_url) = last_art_url_for_updates.lock() {
                        *last_url = info.art_url.clone();
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
    content_column: gtk::Box,
    clamp: adw::Clamp,
    album_art: gtk::Picture,
    placeholder_label: gtk::Label,
    art_container: gtk::Box,
    title_label: gtk::Label,
    artist_label: gtk::Label,
    album_label: gtk::Label,
    play_pause_button: ProgressRingButton,
    prev_button: gtk::Button,
    next_button: gtk::Button,
    volume_scale: gtk::Scale,
    volume_clamp: adw::Clamp,
    volume_updating: Arc<AtomicBool>,
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
        .margin_bottom(0)
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
        .width_request(180)
        .height_request(180)
        .css_classes(vec!["album-art"])
        .build();

    let placeholder_label = gtk::Label::builder()
        .label("?")
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .vexpand(true)
        .hexpand(true)
        .css_classes(vec!["album-art", "album-art-placeholder"])
        .build();

    art_container.append(&album_art);
    art_container.append(&placeholder_label);

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
        .margin_bottom(12)
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

    clamp.set_child(Some(&container));

    // Volume slider — native GNOME look, accent-colored, hidden when not controllable.
    // Wrapped in its own clamp so it matches the width of the album/info area above.
    let volume_adjustment = gtk::Adjustment::new(0.0, 0.0, 1.0, 0.01, 0.05, 0.0);
    let volume_scale = gtk::Scale::builder()
        .orientation(gtk::Orientation::Horizontal)
        .adjustment(&volume_adjustment)
        .draw_value(false)
        .hexpand(true)
        .halign(gtk::Align::Fill)
        .css_classes(vec!["accent"])
        .tooltip_text("Volume")
        .build();

    let volume_clamp = adw::Clamp::builder()
        .maximum_size(280)
        .tightening_threshold(200)
        .margin_top(6)
        .margin_bottom(6)
        .build();
    let volume_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .margin_start(12)
        .margin_end(12)
        .build();
    volume_box.append(&volume_scale);
    volume_clamp.set_child(Some(&volume_box));

    // Outer column: clamped content (expands) + volume + controls (anchored to bottom)
    let content_column = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .vexpand(true)
        .hexpand(true)
        .build();
    content_column.append(&clamp);
    content_column.append(&volume_clamp);
    content_column.append(&controls_box);

    // Hidden until a controllable player is detected.
    volume_clamp.set_visible(false);

    art_container.set_visible(false);
    placeholder_label.set_visible(false);

    MediaContent {
        container,
        content_column,
        clamp,
        album_art,
        placeholder_label,
        art_container,
        title_label,
        artist_label,
        album_label,
        play_pause_button,
        prev_button,
        next_button,
        volume_scale,
        volume_clamp,
        volume_updating: Arc::new(AtomicBool::new(false)),
    }
}

fn update_ui_widgets(
    title_label: &gtk::Label,
    artist_label: &gtk::Label,
    album_label: &gtk::Label,
    album_art: &gtk::Picture,
    placeholder_label: &gtk::Label,
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

    // Update placeholder text: first letter of artist, or title, or "?"
    let initial = if !info.artist.is_empty() {
        info.artist.chars().next().unwrap_or('?').to_uppercase().to_string()
    } else if !info.title.is_empty() {
        info.title.chars().next().unwrap_or('?').to_uppercase().to_string()
    } else {
        "?".to_string()
    };
    placeholder_label.set_text(&initial);

    // Handle album art loading with better error handling - only update when forced
    if force_art_update {
        let has_art = info.art_url.as_ref().map_or(false, |u| !u.is_empty());

        if !has_art {
            // No art URL — show placeholder
            album_art.set_paintable(gtk::gdk::Paintable::NONE);
            album_art.set_visible(false);
            placeholder_label.set_visible(true);
            art_container.set_visible(true);
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
                                        album_art.set_visible(true);
                                        placeholder_label.set_visible(false);
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
                                        album_art.set_visible(false);
                                        placeholder_label.set_visible(true);
                                        art_container.set_visible(true);
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to read bytes from web {}: {}", art_url, e);
                                album_art.set_paintable(gtk::gdk::Paintable::NONE);
                                album_art.set_visible(false);
                                placeholder_label.set_visible(true);
                                art_container.set_visible(true);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to download image from web {}: {}", art_url, e);
                        album_art.set_paintable(gtk::gdk::Paintable::NONE);
                        album_art.set_visible(false);
                        placeholder_label.set_visible(true);
                        art_container.set_visible(true);
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
                            album_art.set_visible(true);
                            placeholder_label.set_visible(false);
                            art_container.set_visible(true);
                            album_art.queue_draw();
                            eprintln!("Successfully loaded art from file: {}", decoded_path);
                        }
                        Err(e) => {
                            eprintln!("Failed to load pixbuf from {}: {}", decoded_path, e);
                            album_art.set_paintable(gtk::gdk::Paintable::NONE);
                            album_art.set_visible(false);
                            placeholder_label.set_visible(true);
                            art_container.set_visible(true);
                        }
                    },
                    false => {
                        eprintln!("Art file does not exist: {}", decoded_path);
                        album_art.set_paintable(gtk::gdk::Paintable::NONE);
                        album_art.set_visible(false);
                        placeholder_label.set_visible(true);
                        art_container.set_visible(true);
                    }
                }
            }
        } else {
            // No art URL provided, show placeholder
            album_art.set_paintable(gtk::gdk::Paintable::NONE);
            album_art.set_visible(false);
            placeholder_label.set_visible(true);
            art_container.set_visible(true);
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

    // Volume slider → MPRIS
    // Track the user's drag with a GestureClick so polling doesn't fight the pointer.
    let volume_updating = content.volume_updating.clone();
    let drag_click = gtk::GestureClick::new();
    drag_click.set_button(0); // listen for any button
    let volume_updating_drag = volume_updating.clone();
    drag_click.connect_pressed(move |_, _, _, _| {
        volume_updating_drag.store(true, Ordering::SeqCst);
    });
    let volume_updating_release = volume_updating.clone();
    drag_click.connect_released(move |_, _, _, _| {
        volume_updating_release.store(false, Ordering::SeqCst);
    });
    let volume_updating_cancel = volume_updating.clone();
    drag_click.connect_cancel(move |_, _| {
        volume_updating_cancel.store(false, Ordering::SeqCst);
    });
    content.volume_scale.add_controller(drag_click);

    content.volume_scale.connect_value_changed({
        let client = client.clone();
        move |scale| {
            let _ = client.set_volume(scale.value());
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
