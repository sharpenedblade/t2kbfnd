use anyhow::{Context, Result};
use glob::glob;
use input_linux::{evdev::EvdevHandle, Event, Key};
use std::{fs::File, io::Write, mem::MaybeUninit};

const TOUCHBAR_CONTROL_PATH: &str = "/sys/bus/hid/drivers/hid-appletb-kbd/*05AC:8302*";
const KEYBOARD_EVENT_PATH: &str = "/dev/input/by-id/*Apple_Internal_Keyboard*event-kbd";

#[derive(Debug, Copy, Clone)]
enum TouchbarMode {
    FUNCTION = 1,
    MEDIA = 2,
}

fn main() {
    let mut touchbar_mode_fd = get_touchbar_mode_fd().unwrap();
    let evdev_handle = get_keyboard_event_fd().unwrap();

    let mut ev_buf = [MaybeUninit::uninit(); 8];
    let mut touchbar_state = TouchbarMode::MEDIA;

    loop {
        let events = evdev_handle
            .read_input_events(&mut ev_buf)
            .unwrap()
            .iter()
            .map(|e| Event::new(e.clone()).unwrap());
        for event in events {
            handle_fn_key(&event, &mut touchbar_state, &mut touchbar_mode_fd);
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

fn handle_fn_key(event: &Event, touchbar_state: &mut TouchbarMode, toucbar_mode_fd: &mut File) {
    if let Event::Key(key_event) = event {
        if let Key::Fn = key_event.key {
            if key_event.value.is_pressed() {
                *touchbar_state = TouchbarMode::FUNCTION;
            } else {
                *touchbar_state = TouchbarMode::MEDIA;
            }
            write_touchbar_mode(toucbar_mode_fd, *touchbar_state);
        }
    }
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
