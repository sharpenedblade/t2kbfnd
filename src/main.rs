use anyhow::{anyhow, Context, Result};
use glob::glob;
use input_linux::{evdev::EvdevHandle, Event, Key};
use std::{
    fs::File,
    io::{Read, Write},
    mem::MaybeUninit,
    time::Instant,
};

const TOUCHBAR_CONTROL_PATH: &str = "/sys/bus/hid/drivers/hid-appletb-kbd/*05AC:8302*";
const KEYBOARD_EVENT_PATH: &str = "/dev/input/by-id/*Apple_Internal_Keyboard*event-kbd";

#[derive(Debug, Copy, Clone)]
enum TouchbarMode {
    FUNCTION = 1,
    MEDIA = 2,
}

struct Touchbar {
    fd: File,
    state: TouchbarMode,
}

impl Touchbar {
    fn new() -> Result<Self> {
        let mut tb_dir = glob(TOUCHBAR_CONTROL_PATH)?
            .next()
            .context("Internal Keyboard not found")??;
        tb_dir.push("mode");

        let mut fd = File::open(tb_dir)?;
        let mut buf = String::new();
        fd.read_to_string(&mut buf).unwrap();

        let state = match buf.as_str() {
            "1" => TouchbarMode::FUNCTION,
            "2" => TouchbarMode::MEDIA,
            _ => return Err(anyhow!("Touchbar state unknown")),
        };
        Ok(Self { fd, state })
    }

    fn set_mode(&mut self, mode: TouchbarMode) -> Result<()> {
        self.fd.write_all(format!("{}", mode as u32).as_bytes())?;
        self.state = mode;
        Ok(())
    }
}

#[derive(Debug, Copy, Clone)]
enum TbBacklightMode {
    OFF = 0,
    DIM = 1,
    MAX = 2,
}

struct TbBacklight {
    pub state: TbBacklightMode,
    fd: File,
}

impl TbBacklight {
    fn new() -> Result<Self> {
        let mut fd = File::open("/sys/class/backlight/appletb_backlight/brightness")?;
        let mut buf = String::new();
        fd.read_to_string(&mut buf).unwrap();
        let state = match buf.as_str() {
            "0" => TbBacklightMode::OFF,
            "1" => TbBacklightMode::DIM,
            "2" => TbBacklightMode::MAX,
            _ => TbBacklightMode::OFF,
        };
        Ok(Self { state, fd })
    }

    fn set_brightness(&mut self, mode: TbBacklightMode) -> Result<()> {
        self.fd.write_all(format!("{}", mode as u32).as_bytes())?;
        self.state = mode;
        Ok(())
    }
}

fn main() {
    let mut touchbar = Touchbar::new().unwrap();
    let mut touchbar_backlight = TbBacklight::new().unwrap();

    let evdev_handle = get_keyboard_event_fd().unwrap();
    let mut ev_buf = [MaybeUninit::uninit(); 8];

    let mut last_event_time = Instant::now();

    loop {
        let events = evdev_handle
            .read_input_events(&mut ev_buf)
            .unwrap()
            .iter()
            .map(|e| Event::new(e.clone()).unwrap());
        for event in events {
            if let Event::Key(key_event) = event {
                if let Key::Fn = key_event.key {
                    if key_event.value.is_pressed() {
                        touchbar.set_mode(TouchbarMode::FUNCTION).unwrap();
                    } else {
                        touchbar.set_mode(TouchbarMode::MEDIA).unwrap();
                    }
                }
            }
            last_event_time = Instant::now();
        }

        let inactive_time = last_event_time.elapsed().as_secs();
        if inactive_time >= 60 {
            touchbar_backlight
                .set_brightness(TbBacklightMode::OFF)
                .unwrap();
        } else if inactive_time >= 30 {
            touchbar_backlight
                .set_brightness(TbBacklightMode::DIM)
                .unwrap();
        } else {
            touchbar_backlight
                .set_brightness(TbBacklightMode::MAX)
                .unwrap();
        }
    }
}

fn get_keyboard_event_fd() -> Result<EvdevHandle<File>> {
    let event_path = glob(KEYBOARD_EVENT_PATH)?
        .next()
        .context("Path not found")??;
    let event_fd = File::open(event_path)?;
    Ok(EvdevHandle::new(event_fd))
}
