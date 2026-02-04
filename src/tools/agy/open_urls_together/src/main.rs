use std::{process::Command, thread, time::Duration};
use std::io::stdout;
use std::io::Write;

use rqcommon::utils::runningenv::{RqCoreConfig};

fn main() { // cargo build --release
    let sleep_in_msec : u32 = 300; // 50 was not enough for digg.com and channel9, 100msec was not enough on 2012-07-18, 150mset was not enough on 2012-07-19

    let rqconfig = RqCoreConfig::new();

    open_in_browser("https://24.hu", sleep_in_msec);
    open_in_browser("https://www.ft.com", sleep_in_msec);
    open_in_browser("https://www.napi.hu/", sleep_in_msec);
    open_in_browser("https://www.hwsw.hu", sleep_in_msec);
    open_in_browser("https://www.gamekapocs.hu/PC", sleep_in_msec);
    open_in_browser("https://www.sciencedaily.com/news", sleep_in_msec);
    open_in_browser("https://stockcharts.com/h-sc/ui?s=VXX", sleep_in_msec);
    open_in_browser("https://stockcharts.com/h-sc/ui?s=$SPX", sleep_in_msec);
    open_in_browser("https://www.dailyfx.com/gbp-usd", sleep_in_msec);

    println!("Open Urls Together!");
    let command = Command::new("cmd").arg("/C").arg("dir").output().expect("there was an error");
    stdout().write(&command.stdout).unwrap();
}

fn open_in_browser(url: &str, sleep_in_msec: u32) {
    // The ^ symbol is the escape character* in Cmd.exe (for & \ < > ^ |). It has to be replaced.
    let _command = Command::new("cmd").arg("/c").arg("start").arg(url.replace("^", "^^").replace("&", "^&")).output().expect("there was an error");
    thread::sleep(Duration::from_millis(sleep_in_msec.into()));
}
