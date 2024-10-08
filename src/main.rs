use anyhow::{anyhow, Context, Result};
use clap::Parser;
use glob::glob;
use log::trace;
use simplelog::TermLogger;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{sync::RwLock, time};

#[derive(Parser)]
struct Args {
    #[arg(short, long, action = clap::ArgAction::Count)]
    debug: u8,
}

const TOUCHBAR_BACKLIGHT_PATH: &str = "/sys/class/backlight/appletb_backlight/brightness";
const KEYBOARD_EVENT_PATH: &str = "/dev/input/by-id/*Apple_Internal_Keyboard*event-kbd";
const TRACKPAD_EVENT_PATH: &str = "/dev/input/by-id/*Apple_Internal_Keyboard*event-mouse";

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
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
        let mut read_fd = File::open(TOUCHBAR_BACKLIGHT_PATH)?;
        let mut buf = String::new();
        read_fd.read_to_string(&mut buf)?;

        let fd = OpenOptions::new()
            .write(true)
            .read(false)
            .open(TOUCHBAR_BACKLIGHT_PATH)?;
        let state = match buf.trim() {
            "0" => TbBacklightMode::Off,
            "1" => TbBacklightMode::Dim,
            "2" => TbBacklightMode::Max,
            _ => return Err(anyhow!("Touchbar backlight state unknown")),
        };
        Ok(Self { state, fd })
    }

    fn set_brightness(&mut self, mode: TbBacklightMode) -> Result<()> {
        if self.state != mode {
            trace!("Setting brightness to {}", mode as u32);
            self.fd.write_all(format!("{}", mode as u32).as_bytes())?;
            self.state = mode;
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let log_level = match args.debug {
        0 => simplelog::LevelFilter::Warn,
        1 => simplelog::LevelFilter::Info,
        2 => simplelog::LevelFilter::Debug,
        _ => simplelog::LevelFilter::Trace,
    };
    TermLogger::new(
        log_level,
        simplelog::Config::default(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    );

    let time_lock = Arc::new(RwLock::new(Instant::now()));

    let mut touchbar_backlight = TbBacklight::new()?;
    let backlight_time_lock = time_lock.clone();
    let _backlight_task = tokio::task::spawn(async move {
        let mut interval = time::interval(Duration::from_millis(100));
        let mut failure_counter = 0;
        loop {
            interval.tick().await;
            let inactive_time = backlight_time_lock.read().await.elapsed().as_secs();
            touchbar_backlight
                .set_brightness(if inactive_time >= 60 {
                    TbBacklightMode::Off
                } else if inactive_time >= 30 {
                    TbBacklightMode::Dim
                } else {
                    TbBacklightMode::Max
                })
                .unwrap_or_else(|_| failure_counter += 1);
            if failure_counter >= 3 {
                return;
            }
        }
    });

    let keyboard_events = get_event_fd(KEYBOARD_EVENT_PATH)?.into_event_stream()?;
    let _keyboard_event_task = create_event_moniter(keyboard_events, time_lock.clone());

    let trackpad_events = get_event_fd(TRACKPAD_EVENT_PATH)?.into_event_stream()?;
    let _trackpad_event_task = create_event_moniter(trackpad_events, time_lock.clone());

    Ok(())
}

fn create_event_moniter(
    mut events: evdev::EventStream,
    time_lock: Arc<RwLock<Instant>>,
) -> tokio::task::JoinHandle<Result<()>> {
    tokio::task::spawn(async move {
        loop {
            let _event = events.next_event().await.unwrap();
            let mut event_time = time_lock.write().await;
            *event_time = Instant::now();
        }
    })
}

fn get_event_fd(p: &str) -> Result<evdev::Device> {
    let event_path = glob(p)?.next().context("Path not found")??;
    let device = evdev::Device::open(event_path)?;
    Ok(device)
}
