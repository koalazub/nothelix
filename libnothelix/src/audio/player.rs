use std::path::PathBuf;
use std::sync::OnceLock;
use which::which;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Player {
    Afplay,
    PwPlay,
    PaPlay,
    Ffplay,
    Aplay,
}

impl Player {
    pub fn binary(self) -> &'static str {
        match self {
            Self::Afplay => "afplay",
            Self::PwPlay => "pw-play",
            Self::PaPlay => "paplay",
            Self::Ffplay => "ffplay",
            Self::Aplay => "aplay",
        }
    }

    pub fn args(self, path: &str) -> Vec<String> {
        match self {
            Self::Ffplay => vec![
                "-nodisp".to_string(),
                "-autoexit".to_string(),
                "-loglevel".to_string(),
                "error".to_string(),
                path.to_string(),
            ],
            _ => vec![path.to_string()],
        }
    }
}

const CANDIDATES: &[Player] = &[
    Player::Afplay,
    Player::PwPlay,
    Player::PaPlay,
    Player::Ffplay,
    Player::Aplay,
];

fn discover() -> Option<(Player, PathBuf)> {
    CANDIDATES.iter().find_map(|candidate| {
        which(candidate.binary())
            .ok()
            .map(|path| (*candidate, path))
    })
}

pub fn resolved() -> Option<&'static (Player, PathBuf)> {
    static PLAYER: OnceLock<Option<(Player, PathBuf)>> = OnceLock::new();
    PLAYER.get_or_init(discover).as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffplay_runs_headless_and_exits_on_finish() {
        assert_eq!(
            Player::Ffplay.args("/tmp/clip.wav"),
            vec![
                "-nodisp".to_string(),
                "-autoexit".to_string(),
                "-loglevel".to_string(),
                "error".to_string(),
                "/tmp/clip.wav".to_string(),
            ]
        );
    }

    #[test]
    fn the_plain_players_take_only_the_path() {
        for player in [
            Player::Afplay,
            Player::PwPlay,
            Player::PaPlay,
            Player::Aplay,
        ] {
            assert_eq!(
                player.args("/tmp/clip.wav"),
                vec!["/tmp/clip.wav".to_string()]
            );
        }
    }

    #[test]
    fn each_player_reports_its_binary_name() {
        assert_eq!(Player::Afplay.binary(), "afplay");
        assert_eq!(Player::PwPlay.binary(), "pw-play");
        assert_eq!(Player::PaPlay.binary(), "paplay");
        assert_eq!(Player::Ffplay.binary(), "ffplay");
        assert_eq!(Player::Aplay.binary(), "aplay");
    }
}
