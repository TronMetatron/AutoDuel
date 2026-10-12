//! Shows the game's software framebuffer as one texture, letterboxed. The classic 320x200
//! picture is shown at whole multiples with sharp pixels; the enhanced graphics (a 1280x800
//! frame) fill the window and are filtered smoothly.
use crate::CardulApp;
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::PrimaryWindow;

/// The C64 screen in its own pixels; the window shows whole multiples of it when it can.
const BASE: UVec2 = UVec2::new(cardul_ui::fb::W as u32, cardul_ui::fb::H as u32);

pub struct DisplayPlugin;

impl Plugin for DisplayPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::BLACK))
            .add_systems(Startup, setup)
            .add_systems(Update, (upload, layout));
    }
}

#[derive(Resource)]
struct Screen {
    image: Handle<Image>,
    /// game tick of the frame in the texture
    shown: Option<u64>,
    /// the frame is high-resolution art (scale > 1): smooth filtering, any size
    smooth: bool,
}

#[derive(Component)]
struct ScreenSprite;

fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>, mut game: NonSendMut<CardulApp>) {
    let (w, h) = game.frame_size();
    let image = images.add(new_image(w.max(1) as u32, h.max(1) as u32));
    commands.spawn((Camera2d, Msaa::Off));
    commands.spawn((Sprite::from_image(image.clone()), ScreenSprite));
    commands.insert_resource(Screen { image, shown: None, smooth: false });
}

fn new_image(w: u32, h: u32) -> Image {
    let mut img = Image::new_fill(
        extent(w, h),
        TextureDimension::D2,
        &[0, 0, 0, 255],
        // sRGB so the palette's byte values show as those colours
        TextureFormat::Rgba8UnormSrgb,
        // keep the pixels on the CPU side too; they are rewritten every game tick
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    img.sampler = ImageSampler::nearest();
    img
}

fn extent(w: u32, h: u32) -> Extent3d {
    Extent3d { width: w, height: h, depth_or_array_layers: 1 }
}

/// Draw the game and copy the frame into the texture (only when a tick has run since the last
/// copy: nothing on screen changes between ticks).
fn upload(mut game: NonSendMut<CardulApp>, mut screen: ResMut<Screen>, mut images: ResMut<Assets<Image>>) {
    let tick = game.tick_count();
    if screen.shown == Some(tick) {
        return;
    }
    let Some(mut image) = images.get_mut(&screen.image) else { return };
    screen.shown = Some(tick);
    let frame = game.draw();
    let (w, h) = (frame.pw.max(1) as u32, frame.ph.max(1) as u32);
    if image.size() != UVec2::new(w, h) {
        image.resize(extent(w, h));
    }
    // the graphics were switched (F10): sharp pixels for the classic art, smooth for the rest
    let smooth = frame.scale > 1;
    if screen.smooth != smooth {
        screen.smooth = smooth;
        image.sampler = if smooth { ImageSampler::linear() } else { ImageSampler::nearest() };
    }
    match image.data.as_mut() {
        Some(d) if d.len() == frame.px.len() => d.copy_from_slice(&frame.px),
        _ => image.data = Some(frame.px.clone()),
    }
}

/// Size and place the sprite: centred, 320:200, the largest whole multiple of 320x200 that fits
/// (in physical pixels), or the largest fractional size when even 1x does not fit.
fn layout(
    windows: Query<&Window, With<PrimaryWindow>>,
    screen: Option<Res<Screen>>,
    mut sprite: Query<(&mut Sprite, &mut Transform), With<ScreenSprite>>,
) {
    let whole = !screen.map(|s| s.smooth).unwrap_or(false);
    let (Ok(win), Ok((mut sprite, mut tf))) = (windows.single(), sprite.single_mut()) else { return };
    let phys = UVec2::new(win.physical_width(), win.physical_height());
    if phys.x == 0 || phys.y == 0 {
        return; // minimised
    }
    let size = fit(phys, BASE, whole);
    // put the top-left corner on a whole physical pixel so every texel gets the same width
    let off = Vec2::new(align(phys.x as f32, size.x), -align(phys.y as f32, size.y));
    // the 2D camera's world units are logical pixels
    let sf = win.scale_factor();
    let custom = Some(size / sf);
    let pos = (off / sf).extend(0.0);
    if sprite.custom_size != custom {
        sprite.custom_size = custom;
    }
    if tf.translation != pos {
        tf.translation = pos;
    }
}

/// Size (physical pixels) to show a `base`-sized screen in a `win`-sized window: a whole
/// multiple when `whole` and one fits, else the largest size that fits.
fn fit(win: UVec2, base: UVec2, whole: bool) -> Vec2 {
    let k = (win.x / base.x).min(win.y / base.y);
    if k >= 1 && whole {
        (base * k).as_vec2()
    } else {
        let f = (win.x as f32 / base.x as f32).min(win.y as f32 / base.y as f32);
        (base.as_vec2() * f).floor().max(Vec2::ONE)
    }
}

/// Shift that moves the leading edge of a centred span onto a pixel boundary (0 or -0.5).
fn align(win: f32, span: f32) -> f32 {
    let edge = (win - span) / 2.0;
    edge.floor() - edge
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fitting() {
        assert_eq!(fit(UVec2::new(1280, 800), BASE, true), Vec2::new(1280.0, 800.0));
        assert_eq!(fit(UVec2::new(1920, 1080), BASE, true), Vec2::new(1600.0, 1000.0));
        assert_eq!(fit(UVec2::new(2560, 1440), BASE, true), Vec2::new(2240.0, 1400.0));
        assert_eq!(fit(UVec2::new(1000, 300), BASE, true), Vec2::new(320.0, 200.0));
        assert_eq!(fit(UVec2::new(200, 400), BASE, true), Vec2::new(200.0, 125.0));
        assert_eq!(fit(UVec2::new(1920, 1080), BASE, false), Vec2::new(1728.0, 1080.0));
        assert_eq!(fit(UVec2::new(2560, 1440), BASE, false), Vec2::new(2304.0, 1440.0));
        assert_eq!(align(1281.0, 1280.0), -0.5);
        assert_eq!(align(1920.0, 1600.0), 0.0);
    }
}
