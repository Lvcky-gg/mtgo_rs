//! Short audio cues inferred only from the viewer's projected game state.
use mtg_core::Zone;
use mtg_engine::PlayerView;
use std::{
    io::Cursor,
    sync::{
        Arc,
        atomic::{AtomicU8, AtomicU32, Ordering},
        mpsc::{self, SyncSender},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    Draw,
    Play,
    Cast,
    Resolve,
    Damage,
    YourTurn,
}

impl Cue {
    fn bytes(self) -> &'static [u8] {
        match self {
            Self::Draw => include_bytes!("../../../assets/sounds/card-draw.wav"),
            Self::Play => include_bytes!("../../../assets/sounds/card-play.wav"),
            Self::Cast => include_bytes!("../../../assets/sounds/spell-cast.wav"),
            Self::Resolve => include_bytes!("../../../assets/sounds/spell-resolve.wav"),
            Self::Damage => include_bytes!("../../../assets/sounds/damage.wav"),
            Self::YourTurn => include_bytes!("../../../assets/sounds/your-turn.wav"),
        }
    }
}

/// Choose a single cue for a state update, favoring the most useful feedback.
/// This intentionally describes observable changes, not undisclosed engine events.
pub fn cue_for(before: &PlayerView, after: &PlayerView) -> Option<Cue> {
    if after.turn < before.turn || before.viewer != after.viewer {
        return None;
    }
    if after.active_player == after.viewer
        && (after.turn != before.turn || after.active_player != before.active_player)
    {
        return Some(Cue::YourTurn);
    }
    if after.players.iter().any(|(id, now)| {
        before
            .players
            .get(id)
            .is_some_and(|old| now.life < old.life)
    }) || after.visible.iter().any(|(id, now)| {
        before
            .visible
            .get(id)
            .is_some_and(|old| now.damage > old.damage)
    }) {
        return Some(Cue::Damage);
    }
    if after.stack.iter().any(|id| {
        !before.stack.contains(id) && after.visible.get(id).is_some_and(|o| !o.is_ability)
    }) {
        return Some(Cue::Cast);
    }
    if after.visible.iter().any(|(id, now)| {
        now.zone.zone == Zone::Battlefield
            && !before
                .visible
                .get(id)
                .is_some_and(|old| old.zone.zone == Zone::Battlefield)
    }) {
        return Some(Cue::Play);
    }
    // Includes countered spells: a quiet cue indicates that something left the stack.
    if before.stack.iter().any(|id| !after.stack.contains(id)) {
        return Some(Cue::Resolve);
    }
    if before
        .players
        .get(&after.viewer)
        .zip(after.players.get(&after.viewer))
        .is_some_and(|(old, now)| {
            now.hand_size > old.hand_size && now.library_size < old.library_size
        })
    {
        return Some(Cue::Draw);
    }
    None
}

#[derive(Debug)]
enum Command {
    Play(Cue),
    RefreshVolume,
}

pub struct SoundFx {
    pub muted: bool,
    pub volume: f32,
    sender: Option<SyncSender<Command>>,
    status: Arc<AtomicU8>,
    gain: Arc<AtomicU32>,
    last: Option<Instant>,
}

impl Default for SoundFx {
    fn default() -> Self {
        Self {
            // Headless unit tests must not open a physical device or make sounds.
            muted: cfg!(test),
            volume: 0.5,
            sender: None,
            status: Arc::new(AtomicU8::new(0)),
            gain: Arc::new(AtomicU32::new(0.5_f32.to_bits())),
            last: None,
        }
    }
}

impl SoundFx {
    pub fn unavailable(&self) -> bool {
        self.status.load(Ordering::Relaxed) == 3
    }

    pub fn play(&mut self, cue: Cue) {
        if self.muted || self.volume <= 0.0 || self.unavailable() {
            return;
        }
        if self
            .last
            .is_some_and(|last| last.elapsed() < Duration::from_millis(90))
        {
            return;
        }
        self.last = Some(Instant::now());
        if self.sender.is_none() {
            let (sender, receiver) = mpsc::sync_channel(8);
            let status = self.status.clone();
            let gain = self.gain.clone();
            gain.store(self.volume.to_bits(), Ordering::Relaxed);
            status.store(1, Ordering::Relaxed);
            std::thread::spawn(move || {
                let Ok(mut device) = rodio::DeviceSinkBuilder::open_default_sink() else {
                    status.store(3, Ordering::Relaxed);
                    return;
                };
                device.log_on_drop(false);
                let player = rodio::Player::connect_new(device.mixer());
                player.set_volume(f32::from_bits(gain.load(Ordering::Relaxed)));
                status.store(2, Ordering::Relaxed);
                while let Ok(command) = receiver.recv() {
                    let volume = f32::from_bits(gain.load(Ordering::Relaxed));
                    player.set_volume(volume);
                    if volume == 0.0 {
                        player.clear();
                    }
                    match command {
                        Command::RefreshVolume => {}
                        Command::Play(cue) => {
                            // Drop bursts instead of queuing delayed or overlapping feedback.
                            if player.volume() == 0.0 || !player.empty() {
                                continue;
                            }
                            if let Ok(source) = rodio::Decoder::try_from(Cursor::new(cue.bytes())) {
                                player.play();
                                player.append(source);
                            }
                        }
                    }
                }
            });
            self.sender = Some(sender);
        }
        if let Some(sender) = &self.sender {
            let _ = sender.try_send(Command::Play(cue));
        }
    }

    pub fn update_volume(&self) {
        self.gain.store(
            (if self.muted { 0.0 } else { self.volume }).to_bits(),
            Ordering::Relaxed,
        );
        if let Some(sender) = &self.sender {
            let _ = sender.try_send(Command::RefreshVolume);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_core::{CardId, PlayerId, ZoneRef};
    use mtg_engine::{state::GameState, view::project};

    fn state() -> GameState {
        GameState::new(&[PlayerId(0), PlayerId(1)], 20)
    }

    #[test]
    fn packaged_cues_decode_to_short_non_silent_audio() {
        for cue in [
            Cue::Draw,
            Cue::Play,
            Cue::Cast,
            Cue::Resolve,
            Cue::Damage,
            Cue::YourTurn,
        ] {
            let source = rodio::Decoder::try_from(Cursor::new(cue.bytes())).unwrap();
            let samples: Vec<_> = source.collect();
            assert!(!samples.is_empty() && samples.len() < 44100);
            assert!(samples.iter().all(|v| v.is_finite() && v.abs() < 0.5));
            assert!(samples.iter().any(|v| v.abs() > 0.05));
        }
    }

    #[test]
    fn draws_require_both_hand_gain_and_library_loss_for_the_viewer() {
        let before = project(&state(), PlayerId(0));
        let mut after = before.clone();
        after.players.get_mut(&PlayerId(0)).unwrap().hand_size += 1;
        assert_eq!(cue_for(&before, &after), None);
        after.players.get_mut(&PlayerId(0)).unwrap().library_size = 9;
        let mut before = before;
        before.players.get_mut(&PlayerId(0)).unwrap().library_size = 10;
        assert_eq!(cue_for(&before, &after), Some(Cue::Draw));
        let mut other = before.clone();
        other.players.get_mut(&PlayerId(1)).unwrap().hand_size += 1;
        assert_eq!(cue_for(&before, &other), None);
    }

    #[test]
    fn stack_and_battlefield_transitions_have_distinct_cues() {
        let mut state = state();
        let before = project(&state, PlayerId(0));
        let object = state.place(CardId(1), PlayerId(0), ZoneRef::shared(Zone::Stack));
        let cast = project(&state, PlayerId(0));
        assert_eq!(cue_for(&before, &cast), Some(Cue::Cast));
        state
            .zone_order
            .get_mut(&ZoneRef::shared(Zone::Stack))
            .unwrap()
            .clear();
        state.objects.remove(&object);
        let resolved = project(&state, PlayerId(0));
        assert_eq!(cue_for(&cast, &resolved), Some(Cue::Resolve));
        state.place(CardId(1), PlayerId(0), ZoneRef::shared(Zone::Battlefield));
        let entered = project(&state, PlayerId(0));
        assert_eq!(cue_for(&resolved, &entered), Some(Cue::Play));
        assert_eq!(cue_for(&entered, &entered), None);
    }

    #[test]
    fn own_turn_and_damage_take_precedence_and_rewinds_stay_quiet() {
        let before = project(&state(), PlayerId(0));
        let mut after = before.clone();
        after.turn += 1;
        after.players.get_mut(&PlayerId(0)).unwrap().life -= 3;
        assert_eq!(cue_for(&before, &after), Some(Cue::YourTurn));
        assert_eq!(cue_for(&after, &before), None);
        after.turn = before.turn;
        assert_eq!(cue_for(&before, &after), Some(Cue::Damage));
    }

    #[test]
    fn mute_updates_shared_gain_even_when_notifications_are_full() {
        let (sender, _receiver) = mpsc::sync_channel(1);
        let mut sound = SoundFx {
            sender: Some(sender),
            ..Default::default()
        };
        sound
            .sender
            .as_ref()
            .unwrap()
            .try_send(Command::RefreshVolume)
            .unwrap();
        sound.muted = true;
        sound.update_volume();
        assert_eq!(f32::from_bits(sound.gain.load(Ordering::Relaxed)), 0.0);
        sound.muted = false;
        sound.volume = 0.25;
        sound.update_volume();
        assert_eq!(f32::from_bits(sound.gain.load(Ordering::Relaxed)), 0.25);
    }

    #[test]
    #[ignore = "requires a deliberately unavailable audio output"]
    fn missing_audio_device_does_not_prevent_gameplay() {
        let mut sound = SoundFx {
            muted: false,
            ..Default::default()
        };
        sound.play(Cue::Play);
        let deadline = Instant::now() + Duration::from_secs(3);
        while sound.status.load(Ordering::Relaxed) == 1 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(sound.unavailable());
        sound.play(Cue::Damage);
        assert!(sound.unavailable());
    }

    #[test]
    #[ignore = "requires an audio output device or ALSA null configuration"]
    fn audio_worker_opens_output_and_accepts_a_cue() {
        let mut sound = SoundFx {
            muted: false,
            ..Default::default()
        };
        sound.play(Cue::Play);
        let deadline = Instant::now() + Duration::from_secs(3);
        while sound.status.load(Ordering::Relaxed) == 1 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(sound.status.load(Ordering::Relaxed), 2);
        sound.muted = true;
        sound.update_volume();
    }
}
