// Now-playing and transport controls of whatever app Windows lists as the current media session (Spotify, a
// browser, the Media Player…), through the System Media Transport Controls.
use serde::Serialize;

#[derive(Serialize, Clone, Default, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MediaState {
    pub available: bool,
    pub app: String,
    pub title: String,
    pub artist: String,
    pub playing: bool,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub can_seek: bool,
    pub can_next: bool,
    pub can_previous: bool,
}

// "SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify", "Spotify.exe", "MSEdge" -> a short display name.
pub fn app_name(aumid: &str) -> String {
    let s = aumid.rsplit('!').next().unwrap_or(aumid);
    let s = s.strip_suffix(".exe").or_else(|| s.strip_suffix(".EXE")).unwrap_or(s);
    s.rsplit('.').next().unwrap_or(s).to_string()
}

#[cfg(windows)]
mod imp {
    use anyhow::{bail, Result};
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    };
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

    use super::{app_name, MediaState};

    // 100 ns ticks since 1601, the clock of WinRT DateTime.
    fn now_ticks() -> i64 {
        let unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() / 100).unwrap_or(0) as i64;
        unix + 116_444_736_000_000_000
    }

    fn session() -> Result<Option<Session>> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let manager = Manager::RequestAsync()?.join()?;
        Ok(manager.GetCurrentSession().ok())
    }

    pub fn state() -> Result<MediaState> {
        let Some(s) = session()? else { return Ok(MediaState::default()) };
        let props = s.TryGetMediaPropertiesAsync()?.join()?;
        let info = s.GetPlaybackInfo()?;
        let timeline = s.GetTimelineProperties()?;
        let controls = info.Controls()?;
        let playing = info.PlaybackStatus()? == Status::Playing;
        let ms = |ticks: i64| (ticks.max(0) / 10_000) as u64;
        let duration_ms = ms(timeline.EndTime()?.Duration);
        let mut position_ms = ms(timeline.Position()?.Duration);
        // The position is a snapshot taken at LastUpdatedTime: carry it forward while playing.
        if playing {
            let updated = timeline.LastUpdatedTime()?.UniversalTime;
            let now = now_ticks();
            if updated > 0 && now > updated {
                position_ms += ms(now - updated);
            }
        }
        if duration_ms > 0 {
            position_ms = position_ms.min(duration_ms);
        }
        Ok(MediaState {
            available: true,
            app: app_name(&s.SourceAppUserModelId()?.to_string()),
            title: props.Title()?.to_string(),
            artist: props.Artist()?.to_string(),
            playing,
            position_ms,
            duration_ms,
            can_seek: controls.IsPlaybackPositionEnabled()? && duration_ms > 0,
            can_next: controls.IsNextEnabled()?,
            can_previous: controls.IsPreviousEnabled()?,
        })
    }

    pub fn command(cmd: &str, position_ms: Option<u64>) -> Result<()> {
        let Some(s) = session()? else { bail!("aucune lecture en cours") };
        let accepted = match cmd {
            "toggle" => s.TryTogglePlayPauseAsync()?.join()?,
            "next" => s.TrySkipNextAsync()?.join()?,
            "previous" => s.TrySkipPreviousAsync()?.join()?,
            "seek" => s.TryChangePlaybackPositionAsync(position_ms.unwrap_or(0) as i64 * 10_000)?.join()?,
            _ => bail!("commande inconnue : {cmd}"),
        };
        if !accepted {
            bail!("le lecteur a refusé la commande");
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod imp {
    use super::MediaState;

    pub fn state() -> anyhow::Result<MediaState> {
        Ok(MediaState::default())
    }

    pub fn command(_cmd: &str, _position_ms: Option<u64>) -> anyhow::Result<()> {
        anyhow::bail!("contrôle de la lecture indisponible sur ce système")
    }
}

pub use imp::{command, state};

#[cfg(test)]
mod tests {
    #[test]
    fn app_names_are_short() {
        assert_eq!(super::app_name("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify"), "Spotify");
        assert_eq!(super::app_name("Spotify.exe"), "Spotify");
        assert_eq!(super::app_name("Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic"), "ZuneMusic");
        assert_eq!(super::app_name("MSEdge"), "MSEdge");
    }

    #[test]
    #[ignore]
    fn live_state() {
        // cargo test -- --ignored live_state: reads the current Windows media session.
        println!("PROBE {:?}", super::state());
    }
}
