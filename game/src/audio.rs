//! Sound: small C64-flavoured square/noise cues, synthesised as WAV at startup, and an engine
//! hum that loops while a car is running, its pitch following the revs (`App::engine`).
use crate::CardulApp;
use bevy::audio::{AudioSinkPlayback, Volume};
use bevy::prelude::*;
use cardul_sim::combat::Sfx;

const RATE: u32 = 22050;
/// At most this many copies of one sound play at once.
const MAX_SAME: usize = 4;
const VOLUME: f32 = 0.6;

/// Every cue, in `Sfx as usize` order.
const ALL: [Sfx; 20] = [
    Sfx::Beep,
    Sfx::Click,
    Sfx::Shot,
    Sfx::Rocket,
    Sfx::Laser,
    Sfx::Flame,
    Sfx::Hit,
    Sfx::PlayerHit,
    Sfx::Crash,
    Sfx::Explode,
    Sfx::Fanfare,
    Sfx::Drop,
    Sfx::Key,
    Sfx::Step,
    Sfx::Door,
    Sfx::Cash,
    Sfx::Arrive,
    Sfx::Death,
    Sfx::Title,
    Sfx::Horn,
];

/// The engine loop: `ENGINE_HZ` cycles in `ENGINE_SECS` (a whole number, so it loops cleanly),
/// played `ENGINE_SPEED` times faster from idle to top revs.
const ENGINE_HZ: f32 = 40.0;
const ENGINE_SECS: f32 = 0.5;
const ENGINE_SPEED: (f32, f32) = (0.75, 2.6);
const ENGINE_VOLUME: (f32, f32) = (0.07, 0.18);

pub struct SfxPlugin;

impl Plugin for SfxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SfxQueue>().add_systems(Startup, setup).add_systems(Update, (play, engine));
    }
}

/// Cues from the game ticks, played on the next frame.
#[derive(Resource, Default)]
pub struct SfxQueue(pub Vec<Sfx>);

#[derive(Resource)]
struct SfxBank([Handle<AudioSource>; ALL.len()]);

/// A playing cue (the entity despawns when it finishes).
#[derive(Component)]
struct Voice(usize);

/// The engine loop (always playing; silent when no car is running).
#[derive(Component)]
struct EngineVoice;

fn setup(mut commands: Commands, mut sources: ResMut<Assets<AudioSource>>) {
    let handles = ALL.map(|s| sources.add(AudioSource { bytes: wav(&synth(s)).into() }));
    commands.insert_resource(SfxBank(handles));
    let hum = sources.add(AudioSource { bytes: wav(&engine_loop()).into() });
    commands.spawn((AudioPlayer(hum), PlaybackSettings::LOOP.with_volume(Volume::Linear(0.0)), EngineVoice));
}

/// Follow the game's engine revs: pitch and loudness glide toward the target each frame.
fn engine(game: NonSend<CardulApp>, mut sinks: Query<&mut AudioSink, With<EngineVoice>>) {
    let Ok(mut sink) = sinks.single_mut() else { return };
    let (speed, vol) = match game.engine() {
        Some(r) => (ENGINE_SPEED.0 + (ENGINE_SPEED.1 - ENGINE_SPEED.0) * r, ENGINE_VOLUME.0 + (ENGINE_VOLUME.1 - ENGINE_VOLUME.0) * r),
        None => (sink.speed(), 0.0),
    };
    let cur = sink.speed();
    sink.set_speed(cur + (speed - cur) * 0.12);
    let v = sink.volume().to_linear();
    sink.set_volume(Volume::Linear(v + (vol - v) * 0.2));
}

fn play(mut commands: Commands, bank: Res<SfxBank>, mut queue: ResMut<SfxQueue>, voices: Query<&Voice>) {
    if queue.0.is_empty() {
        return;
    }
    let mut playing = [0usize; ALL.len()];
    for v in &voices {
        playing[v.0] += 1;
    }
    for s in queue.0.drain(..) {
        let i = s as usize;
        if playing[i] >= MAX_SAME {
            continue;
        }
        playing[i] += 1;
        commands.spawn((
            AudioPlayer(bank.0[i].clone()),
            PlaybackSettings::DESPAWN.with_volume(Volume::Linear(VOLUME)),
            Voice(i),
        ));
    }
}

// ---- synthesis ----

fn len(secs: f32) -> usize {
    (secs * RATE as f32) as usize
}

/// Envelope: 2 ms attack, 4 ms release, and `(1 - t)^decay` in between (0 = flat).
fn env(i: usize, n: usize, decay: f32) -> f32 {
    let attack = (i as f32 / len(0.002) as f32).min(1.0);
    let release = ((n - i) as f32 / len(0.004) as f32).min(1.0);
    attack * release * (1.0 - i as f32 / n as f32).powf(decay)
}

/// Square wave sliding from `f0` to `f1` Hz.
fn square(secs: f32, f0: f32, f1: f32, vol: f32, decay: f32) -> Vec<f32> {
    let n = len(secs);
    let mut phase = 0.0f32;
    (0..n)
        .map(|i| {
            let f = f0 + (f1 - f0) * i as f32 / n as f32;
            phase = (phase + f / RATE as f32).fract();
            let v = if phase < 0.5 { 1.0 } else { -1.0 };
            v * vol * env(i, n, decay)
        })
        .collect()
}

/// SID-style noise: a new random level `c0`..`c1` times a second (lower = rumblier).
fn noise(secs: f32, c0: f32, c1: f32, vol: f32, decay: f32, seed: u32) -> Vec<f32> {
    let n = len(secs);
    let mut phase = 0.0f32;
    let mut rng = seed.wrapping_mul(2_654_435_761).max(1);
    let mut level = 0.0f32;
    (0..n)
        .map(|i| {
            let c = c0 + (c1 - c0) * i as f32 / n as f32;
            phase += c / RATE as f32;
            if phase >= 1.0 || i == 0 {
                phase = phase.fract();
                // xorshift32
                rng ^= rng << 13;
                rng ^= rng >> 17;
                rng ^= rng << 5;
                level = (rng >> 8) as f32 / (1u32 << 23) as f32 - 1.0;
            }
            level * vol * env(i, n, decay)
        })
        .collect()
}

fn mix(a: Vec<f32>, b: Vec<f32>) -> Vec<f32> {
    let (mut long, short) = if a.len() >= b.len() { (a, b) } else { (b, a) };
    for (x, y) in long.iter_mut().zip(short) {
        *x += y;
    }
    long
}

fn synth(s: Sfx) -> Vec<f32> {
    match s {
        Sfx::Beep => square(0.08, 1000.0, 1000.0, 0.25, 0.0),
        Sfx::Click => square(0.012, 1800.0, 900.0, 0.3, 1.0),
        Sfx::Shot => noise(0.12, 9000.0, 4000.0, 0.5, 2.0, 1),
        Sfx::Rocket => noise(0.45, 6000.0, 500.0, 0.45, 0.8, 2),
        Sfx::Laser => square(0.16, 4000.0, 1500.0, 0.22, 0.7),
        Sfx::Flame => noise(0.25, 16000.0, 11000.0, 0.22, 0.5, 3),
        Sfx::Hit => square(0.12, 140.0, 60.0, 0.5, 1.5),
        Sfx::PlayerHit => mix(square(0.2, 90.0, 40.0, 0.45, 1.2), noise(0.2, 3000.0, 1500.0, 0.3, 2.0, 4)),
        Sfx::Crash => noise(0.35, 4000.0, 1500.0, 0.55, 1.5, 5),
        Sfx::Explode => noise(1.2, 3000.0, 250.0, 0.6, 2.0, 6),
        Sfx::Fanfare => {
            let mut v = vec![];
            for (i, f) in [523.25, 659.25, 783.99, 1046.5].into_iter().enumerate() {
                let last = i == 3;
                v.extend(square(if last { 0.3 } else { 0.11 }, f, f, 0.22, if last { 1.0 } else { 0.3 }));
                v.extend(std::iter::repeat_n(0.0, len(0.02)));
            }
            v
        }
        Sfx::Drop => square(0.07, 220.0, 150.0, 0.35, 0.8),
        Sfx::Key => square(0.008, 2400.0, 1800.0, 0.12, 2.0),
        Sfx::Step => mix(noise(0.05, 900.0, 300.0, 0.22, 3.0, 7), square(0.04, 90.0, 60.0, 0.12, 2.0)),
        // doorbell
        Sfx::Door => tune(&[(880.0, 0.12), (698.5, 0.22)], 0.16, 1.2),
        // the till: a rattle, then the bell
        Sfx::Cash => mix(noise(0.04, 8000.0, 6000.0, 0.25, 2.0, 8), seq(&[(0.03, square(0.06, 1568.0, 1568.0, 0.16, 0.5)), (0.0, square(0.25, 2093.0, 2093.0, 0.18, 1.3))])),
        Sfx::Arrive => tune(&[(392.0, 0.09), (523.25, 0.09), (659.25, 0.22)], 0.18, 0.8),
        Sfx::Death => {
            let mut v = vec![];
            for (i, f) in [440.0f32, 370.0, 311.1, 261.6].into_iter().enumerate() {
                let last = i == 3;
                v.extend(square(if last { 0.6 } else { 0.2 }, f, if last { f * 0.94 } else { f }, 0.2, if last { 1.0 } else { 0.4 }));
                v.extend(std::iter::repeat_n(0.0, len(0.03)));
            }
            v
        }
        Sfx::Title => title_tune(),
        // two reedy tones a little apart, like an old bus horn
        Sfx::Horn => mix(square(0.55, 311.0, 305.0, 0.13, 0.3), square(0.55, 392.0, 386.0, 0.11, 0.3)),
    }
}

/// Notes (Hz, seconds) one after the other, each with a short gap.
fn tune(notes: &[(f32, f32)], vol: f32, decay: f32) -> Vec<f32> {
    let mut v = vec![];
    for (f, d) in notes {
        v.extend(square(*d, *f, *f, vol, decay));
        v.extend(std::iter::repeat_n(0.0, len(0.015)));
    }
    v
}

/// Sounds one after the other, each followed by `gap` seconds of silence.
fn seq(parts: &[(f32, Vec<f32>)]) -> Vec<f32> {
    let mut v = vec![];
    for (gap, p) in parts {
        v.extend_from_slice(p);
        v.extend(std::iter::repeat_n(0.0, len(*gap)));
    }
    v
}

/// The title tune: a minor-key lead over a pulsing bass, about three seconds.
fn title_tune() -> Vec<f32> {
    const BEAT: f32 = 0.16;
    // (lead Hz, beats); 0 = rest
    let lead: [(f32, f32); 12] = [
        (329.6, 1.0), (440.0, 1.0), (523.3, 1.0), (659.3, 2.0), (587.3, 1.0), (523.3, 1.0),
        (493.9, 1.0), (523.3, 1.0), (440.0, 2.0), (0.0, 1.0), (659.3, 1.0), (880.0, 4.0),
    ];
    let mut melody = vec![];
    for (f, beats) in lead {
        let d = BEAT * beats;
        if f == 0.0 {
            melody.extend(std::iter::repeat_n(0.0, len(d)));
        } else {
            melody.extend(square(d - 0.02, f, f, 0.14, if beats > 2.0 { 1.0 } else { 0.4 }));
            melody.extend(std::iter::repeat_n(0.0, len(0.02)));
        }
    }
    let mut bass = vec![];
    for root in [110.0f32, 87.3, 98.0, 110.0] {
        for _ in 0..4 {
            bass.extend(square(BEAT - 0.01, root, root, 0.12, 1.5));
            bass.extend(std::iter::repeat_n(0.0, len(0.01)));
        }
    }
    mix(melody, bass)
}

/// The engine: a pulse wave with its octave and a little rumble, smoothed, `ENGINE_SECS` long
/// and starting where it ends, for looping.
fn engine_loop() -> Vec<f32> {
    let n = len(ENGINE_SECS);
    let mut rng = 0x1234_5678u32;
    let raw: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let ph = (t * ENGINE_HZ).fract();
            let ph2 = (t * ENGINE_HZ * 2.0).fract();
            let pulse = if ph < 0.3 { 1.0 } else { -0.45 };
            let oct = if ph2 < 0.5 { 0.35 } else { -0.35 };
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            let n = (rng >> 8) as f32 / (1u32 << 23) as f32 - 1.0;
            pulse + oct + 0.25 * n
        })
        .collect();
    // one-pole low-pass, run round the loop twice so the end meets the start
    let mut y = 0.0f32;
    let mut out = vec![0.0; n];
    for pass in 0..2 {
        for i in 0..n {
            y += (raw[i] - y) * 0.18;
            if pass == 1 {
                out[i] = y;
            }
        }
    }
    let peak = out.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-6);
    out.iter().map(|v| v / peak * 0.8).collect()
}

/// 16-bit mono PCM WAV.
fn wav(samples: &[f32]) -> Vec<u8> {
    let data = (samples.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&RATE.to_le_bytes());
    b.extend_from_slice(&(RATE * 2).to_le_bytes()); // bytes per second
    b.extend_from_slice(&2u16.to_le_bytes()); // block align
    b.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        b.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every cue in turn, then the engine revving up and down, as one WAV to listen to:
    /// `cargo test -p cardul --release -- --ignored sound_demo` writes target/sound-demo.wav.
    #[test]
    #[ignore]
    fn sound_demo() {
        let mut out = vec![];
        for s in ALL {
            out.extend(synth(s));
            out.extend(std::iter::repeat_n(0.0, len(0.4)));
        }
        let hum = engine_loop();
        let mut pos = 0.0f32;
        let total = len(6.0);
        for i in 0..total {
            let t = i as f32 / total as f32;
            let r = if t < 0.5 { t * 2.0 } else { (1.0 - t) * 2.0 };
            let speed = ENGINE_SPEED.0 + (ENGINE_SPEED.1 - ENGINE_SPEED.0) * r;
            let vol = ENGINE_VOLUME.0 + (ENGINE_VOLUME.1 - ENGINE_VOLUME.0) * r;
            out.push(hum[pos as usize % hum.len()] * vol / VOLUME);
            pos += speed;
        }
        let out: Vec<f32> = out.iter().map(|v| v * VOLUME).collect();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/sound-demo.wav");
        std::fs::write(&path, wav(&out)).unwrap();
    }

    #[test]
    fn engine_loops() {
        let e = engine_loop();
        assert_eq!(e.len(), len(ENGINE_SECS));
        assert!((ENGINE_HZ * ENGINE_SECS).fract() == 0.0, "whole cycles");
        assert!(e.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
        // no click where the loop wraps: that step is no bigger than the steps inside it
        let inside = e.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
        assert!((e[0] - e[e.len() - 1]).abs() <= inside + 1e-6);
    }

    #[test]
    fn sounds() {
        for (i, s) in ALL.into_iter().enumerate() {
            assert_eq!(s as usize, i);
            let pcm = synth(s);
            assert!(pcm.len() > 100, "{s:?} too short");
            assert!(pcm.iter().all(|v| v.is_finite() && v.abs() <= 1.0), "{s:?} clips");
            assert!(pcm.iter().any(|v| v.abs() > 0.05), "{s:?} silent");
            let w = wav(&pcm);
            assert_eq!(w.len(), 44 + pcm.len() * 2);
            assert_eq!(&w[36..40], b"data");
        }
    }
}
