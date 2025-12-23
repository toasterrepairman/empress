use mpris::{PlayerFinder, Player, PlaybackStatus};
use std::sync::mpsc::{channel, Sender, Receiver};
use std::thread;
use std::time::Duration;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default)]
pub struct MediaInfo {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_url: Option<String>,
    pub status: PlayerStatus,
    pub position: Option<Duration>,
    pub length: Option<Duration>,
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
}

#[derive(Clone)]
pub struct MprisClient {
    command_sender: Sender<Command>,
    preferred_player: Arc<Mutex<Option<String>>>,
}

impl MprisClient {
    pub fn new() -> Self {
        let (command_sender, command_receiver) = channel::<Command>();
        let preferred_player: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

        let preferred_player_clone = preferred_player.clone();

        // Spawn a thread that owns the Player and handles commands
        thread::spawn(move || {
            let mut player: Option<Player> = None;

            loop {
                // Update player reference continuously (before checking commands)
                let finder = PlayerFinder::new();
                if let Ok(finder) = finder {
                    // Check if we have a preferred player
                    let pref = preferred_player_clone.lock().unwrap();
                    if let Some(ref preferred) = *pref {
                        // Try to find the specific player
                        if let Ok(p) = finder.find_by_name(preferred) {
                            player = Some(p);
                        } else if let Some(active_player) = finder.find_active().ok() {
                            player = Some(active_player);
                        } else {
                            player = None;
                        }
                    } else {
                        // No preferred player, use active
                        if let Some(active_player) = finder.find_active().ok() {
                            player = Some(active_player);
                        } else {
                            player = None;
                        }
                    }
                }

                // Check for commands (non-blocking)
                if let Ok(cmd) = command_receiver.try_recv() {
                    if let Some(ref p) = player {
                        let _ = match cmd {
                            Command::PlayPause => p.play_pause(),
                            Command::Next => p.next(),
                            Command::Previous => p.previous(),
                            Command::Seek(offset) => p.seek(offset),
                        };
                    }
                }

                thread::sleep(Duration::from_millis(100));
            }
        });

        Self { command_sender, preferred_player }
    }

    pub fn set_preferred_player(&self, player_name: Option<String>) {
        *self.preferred_player.lock().unwrap() = player_name;
    }

    pub fn get_available_players() -> Vec<String> {
        if let Ok(finder) = PlayerFinder::new() {
            if let Ok(players) = finder.find_all() {
                players.into_iter()
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

    pub fn start_monitoring(&self) -> Receiver<MediaInfo> {
        let (info_sender, info_receiver) = channel();
        let preferred_player = self.preferred_player.clone();

        thread::spawn(move || {
            loop {
                let finder = PlayerFinder::new();

                let info = if let Ok(finder) = finder {
                    // Check if we have a preferred player
                    let pref = preferred_player.lock().unwrap();
                    let player_opt = if let Some(ref preferred) = *pref {
                        // Try to find the specific player first
                        finder.find_by_name(preferred)
                            .ok()
                            .or_else(|| finder.find_active().ok())
                    } else {
                        // No preferred player, use active
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

                thread::sleep(Duration::from_millis(500));
            }
        });

        info_receiver
    }

    fn get_media_info(player: &Player) -> MediaInfo {
        let metadata = player.get_metadata().ok();
        let status = player.get_playback_status().ok()
            .map(PlayerStatus::from)
            .unwrap_or_default();

        let (title, artist, album, art_url) = if let Some(ref m) = metadata {
            (
                m.title().unwrap_or("Unknown").to_string(),
                m.artists().and_then(|a| a.first().map(|s| s.to_string()))
                    .unwrap_or_else(|| "Unknown Artist".to_string()),
                m.album_name().unwrap_or("").to_string(),
                m.art_url().map(|s| s.to_string()),
            )
        } else {
            ("No media playing".to_string(), String::new(), String::new(), None)
        };

        let position = player.get_position().ok();
        let length = metadata.as_ref()
            .and_then(|m| m.length())
            .and_then(|l| Duration::try_from(l).ok());

        MediaInfo {
            title,
            artist,
            album,
            art_url,
            status,
            position,
            length,
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
}
