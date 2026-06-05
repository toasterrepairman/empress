use mpris::{PlaybackStatus, Player, PlayerFinder};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Clone, Debug, Default)]
pub struct MediaInfo {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_url: Option<String>,
    pub status: PlayerStatus,
    pub position: Option<Duration>,
    pub length: Option<Duration>,
    pub volume: Option<f64>,
    pub can_control: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum PlayerStatus {
    #[default]
    Stopped,
    Playing,
    Paused,
}

impl From<PlaybackStatus> for PlayerStatus {
    fn from(status: PlaybackStatus) -> Self {
        match status {
            PlaybackStatus::Playing => PlayerStatus::Playing,
            PlaybackStatus::Paused => PlayerStatus::Paused,
            PlaybackStatus::Stopped => PlayerStatus::Stopped,
        }
    }
}

enum Command {
    PlayPause,
    Next,
    Previous,
    Seek(i64),
    SetVolume(f64),
}

#[derive(Clone)]
pub struct MprisClient {
    command_sender: Sender<Command>,
    preferred_player: Arc<Mutex<Option<String>>>,
    monitor_tick: Sender<()>,
    monitor_tick_receiver: Arc<Mutex<Option<Receiver<()>>>>,
}

impl MprisClient {
    pub fn new() -> Self {
        let (command_sender, command_receiver) = channel::<Command>();
        let preferred_player: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let (monitor_tick, tick_receiver) = channel::<()>();
        let monitor_tick_receiver = Arc::new(Mutex::new(Some(tick_receiver)));

        let preferred_player_clone = preferred_player.clone();

        // Spawn a thread that owns the Player and handles commands
        thread::spawn(move || {
            let mut player: Option<Player> = None;

            loop {
                // Block until a command arrives (no busy D-Bus polling).
                let Ok(cmd) = command_receiver.recv() else {
                    break; // channel closed
                };

                // Resolve the player fresh for each command.
                let preferred_name = preferred_player_clone
                    .lock()
                    .ok()
                    .and_then(|pref| pref.clone());

                if let Ok(finder) = PlayerFinder::new() {
                    player = if let Some(ref preferred) = preferred_name {
                        finder
                            .find_by_name(preferred)
                            .ok()
                            .or_else(|| finder.find_active().ok())
                    } else {
                        finder.find_active().ok()
                    };
                }

                if let Some(ref p) = player {
                    let _ = match cmd {
                        Command::PlayPause => p.play_pause(),
                        Command::Next => p.next(),
                        Command::Previous => p.previous(),
                        Command::Seek(offset) => p.seek(offset),
                        Command::SetVolume(v) => p.set_volume(v.max(0.0)),
                    };
                }
            }
        });

        Self {
            command_sender,
            preferred_player,
            monitor_tick,
            monitor_tick_receiver,
        }
    }

    pub fn set_preferred_player(&self, player_name: Option<String>) {
        *self.preferred_player.lock().unwrap() = player_name;
        // Wake the monitor thread so it picks up the new player immediately
        // instead of waiting for the next 500ms tick.
        let _ = self.monitor_tick.send(());
    }

    /// Take the monitor's tick receiver out. Must be called exactly once,
    /// before `start_monitoring`.
    pub fn take_monitor_tick(&self) -> Option<Receiver<()>> {
        self.monitor_tick_receiver.lock().unwrap().take()
    }

    pub fn get_available_players() -> Vec<String> {
        if let Ok(finder) = PlayerFinder::new() {
            if let Ok(players) = finder.find_all() {
                players
                    .into_iter()
                    .map(|p| p.identity().to_string())
                    .collect()
            } else {
                vec![]
            }
        } else {
            vec![]
        }
    }

    pub fn get_player_name(player: &Player) -> String {
        player.identity().to_string()
    }

    pub fn start_monitoring(&self, tick_receiver: Receiver<()>) -> Receiver<MediaInfo> {
        let (info_sender, info_receiver) = channel();
        let preferred_player = self.preferred_player.clone();

        thread::spawn(move || {
            loop {
                let preferred_name = preferred_player
                    .lock()
                    .ok()
                    .and_then(|pref| pref.clone());

                let info = if let Ok(finder) = PlayerFinder::new() {
                    let player_opt = if let Some(ref preferred) = preferred_name {
                        finder
                            .find_by_name(preferred)
                            .ok()
                            .or_else(|| finder.find_active().ok())
                    } else {
                        finder.find_active().ok()
                    };

                    if let Some(player) = player_opt {
                        Self::get_media_info(&player)
                    } else {
                        MediaInfo::default()
                    }
                } else {
                    MediaInfo::default()
                };

                // Send info through channel; if receiver is dropped, exit thread
                if info_sender.send(info).is_err() {
                    break;
                }

                // Wait up to 500ms for the next tick, or until we're poked.
                // A poke (e.g. channel change) cuts the wait short.
                if tick_receiver.recv_timeout(Duration::from_millis(500)).is_ok() {
                    // Drain any extra pokes that arrived during the wait.
                    while tick_receiver.try_recv().is_ok() {}
                }
            }
        });

        info_receiver
    }

    fn get_media_info(player: &Player) -> MediaInfo {
        let metadata = player.get_metadata().ok();
        let status = player
            .get_playback_status()
            .ok()
            .map(PlayerStatus::from)
            .unwrap_or_default();

        let (title, artist, album, art_url) = if let Some(ref m) = metadata {
            (
                m.title().unwrap_or("Unknown").to_string(),
                m.artists()
                    .and_then(|a| a.first().map(|s| s.to_string()))
                    .unwrap_or_else(|| "Unknown Artist".to_string()),
                m.album_name().unwrap_or("").to_string(),
                m.art_url().map(|s| s.to_string()),
            )
        } else {
            (
                "No media playing".to_string(),
                String::new(),
                String::new(),
                None,
            )
        };

        let position = player.get_position().ok();
        let length = metadata
            .as_ref()
            .and_then(|m| m.length())
            .and_then(|l| Duration::try_from(l).ok());

        let can_control = player.can_control().unwrap_or(false);
        let volume = if can_control {
            player.get_volume().ok()
        } else {
            None
        };

        MediaInfo {
            title,
            artist,
            album,
            art_url,
            status,
            position,
            length,
            volume,
            can_control,
        }
    }

    pub fn play_pause(&self) -> anyhow::Result<()> {
        self.command_sender.send(Command::PlayPause)?;
        Ok(())
    }

    pub fn next(&self) -> anyhow::Result<()> {
        self.command_sender.send(Command::Next)?;
        Ok(())
    }

    pub fn previous(&self) -> anyhow::Result<()> {
        self.command_sender.send(Command::Previous)?;
        Ok(())
    }

    pub fn seek(&self, offset_micros: i64) -> anyhow::Result<()> {
        self.command_sender.send(Command::Seek(offset_micros))?;
        Ok(())
    }

    pub fn set_volume(&self, volume: f64) -> anyhow::Result<()> {
        self.command_sender.send(Command::SetVolume(volume))?;
        Ok(())
    }
}
