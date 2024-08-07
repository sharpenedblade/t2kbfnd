use anyhow::{Context, Result};
use glob::glob;
use input_linux::{evdev::EvdevHandle, Event, Key};
use std::{
    fs::File,
    io::{Read, Write},
    mem::MaybeUninit,
};

const TOUCHBAR_CONTROL_PATH: &str = "/sys/bus/hid/drivers/hid-appletb-kbd/*05AC:8302*";
const KEYBOARD_EVENT_PATH: &str = "/dev/input/by-id/*Apple_Internal_Keyboard*event-kbd";

#[derive(Debug, Copy, Clone)]
enum TouchbarMode {
    FUNCTION = 1,
    MEDIA = 2,
}

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
}

fn main() {
    let mut touchbar_mode_fd = get_touchbar_mode_fd().unwrap();
    let evdev_handle = get_keyboard_event_fd().unwrap();

    let mut ev_buf = [MaybeUninit::uninit(); 8];
    let mut touchbar_state = TouchbarMode::MEDIA;

    let mut touchbar_backlight = TbBacklight::new().unwrap();

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
                        touchbar_state = TouchbarMode::FUNCTION;
                    } else {
                        touchbar_state = TouchbarMode::MEDIA;
                    }
                    write_touchbar_mode(&mut touchbar_mode_fd, touchbar_state);
                }
            }
        }
    }
}

fn get_touchbar_mode_fd() -> Result<File> {
    let mut kb_dir = glob(TOUCHBAR_CONTROL_PATH)?
        .next()
        .context("Internal Keyboard not found")??;
    kb_dir.push("mode");
    Ok(File::open(kb_dir)?)
}

fn write_touchbar_mode(fd: &mut File, mode: TouchbarMode) {
    fd.write_all(format!("{}", mode as u32).as_bytes()).unwrap();
}

fn get_keyboard_event_fd() -> Result<EvdevHandle<File>> {
    let event_path = glob(KEYBOARD_EVENT_PATH)?
        .next()
        .context("Path not found")??;
    let event_fd = File::open(event_path)?;
    Ok(EvdevHandle::new(event_fd))
}
