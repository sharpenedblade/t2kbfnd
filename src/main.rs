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

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum TouchbarMode {
    Function = 1,
    Media = 2,
}

struct Touchbar {
    fd: File,
    state: TouchbarMode,
    default_mode: TouchbarMode,
}

impl Touchbar {
    fn new(default_mode: TouchbarMode) -> Result<Self> {
        let mut tb_dir = glob(TOUCHBAR_CONTROL_PATH)?
            .next()
            .context("Internal Keyboard not found")??;
        tb_dir.push("mode");

        let mut fd = File::open(tb_dir)?;
        let mut buf = String::new();
        fd.read_to_string(&mut buf).unwrap();

        let state = match buf.as_str() {
            "1" => TouchbarMode::Function,
            "2" => TouchbarMode::Media,
            _ => return Err(anyhow!("Touchbar state unknown")),
        };
        Ok(Self {
            default_mode,
            fd,
            state,
        })
    }

    fn set_mode(&mut self, mode: TouchbarMode) -> Result<()> {
        self.fd.write_all(format!("{}", mode as u32).as_bytes())?;
        self.state = mode;
        Ok(())
    }
}

#[derive(Debug, Copy, Clone)]
enum TbBacklightMode {
    Off = 0,
    Dim = 1,
    Max = 2,
}

struct TbBacklight {
    pub state: TbBacklightMode,
    fd: File,
}

impl TbBacklight {
    fn new() -> Result<Self> {
        let mut fd = File::open("/sys/class/backlight/appletb_backlight/brightness")?;
        let mut buf = String::new();
        fd.read_to_string(&mut buf)?;
        let state = match buf.as_str() {
            "0" => TbBacklightMode::Off,
            "1" => TbBacklightMode::Dim,
            "2" => TbBacklightMode::Max,
            _ => return Err(anyhow!("Touchbar backlight state unknown")),
        };
        Ok(Self { state, fd })
    }

    fn set_brightness(&mut self, mode: TbBacklightMode) -> Result<()> {
        self.fd.write_all(format!("{}", mode as u32).as_bytes())?;
        self.state = mode;
        Ok(())
    }
}

fn load_config() -> Result<TouchbarMode> {
    let config = std::fs::read_to_string("/etc/t2kbfnd.txt")?;
    let mode = match config.trim() {
        "media" => TouchbarMode::Media,
        "function" => TouchbarMode::Function,
        _ => return Err(anyhow!("Bad config file")),
    };
    Ok(mode)
}

fn main() {
    let default_mode = load_config().unwrap_or(TouchbarMode::Media);

    let mut touchbar = Touchbar::new(default_mode).unwrap();
    let mut touchbar_backlight = TbBacklight::new().unwrap();

    let evdev_handle = get_keyboard_event_fd().unwrap();
    let mut ev_buf = [MaybeUninit::uninit(); 8];

    let mut last_event_time = Instant::now();

    loop {
        let events = evdev_handle
            .read_input_events(&mut ev_buf)
            .unwrap()
            .iter()
            .map(|e| Event::new(*e).unwrap());
        for event in events {
            if let Event::Key(key_event) = event {
                if key_event.key == Key::Fn {
                    touchbar
                        .set_mode(if key_event.value.is_pressed() {
                            if touchbar.default_mode == TouchbarMode::Media {
                                TouchbarMode::Function
                            } else {
                                TouchbarMode::Media
                            }
                        } else {
                            touchbar.default_mode
                        })
                        .unwrap()
                }
            }
            last_event_time = Instant::now();
        }

        let inactive_time = last_event_time.elapsed().as_secs();
        if inactive_time >= 60 {
            touchbar_backlight
                .set_brightness(TbBacklightMode::Off)
                .unwrap();
        } else if inactive_time >= 30 {
            touchbar_backlight
                .set_brightness(TbBacklightMode::Dim)
                .unwrap();
        } else {
            touchbar_backlight
                .set_brightness(TbBacklightMode::Max)
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
