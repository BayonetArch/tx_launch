/*
 * tx_launch
 * Copyright (c) 2025 BayonetArch
 *
 * This software is released under the MIT License.
 * See LICENSE file for details.
 */

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{from_str, to_vec_pretty};
use simple_term_attr::{StyleAttributes, clear_line, clear_screen};
use spb::progress_bar;
use std::{
    collections::{HashMap, HashSet},
    env::{self},
    fs::{self, DirBuilder},
    io::{self, Write, stdout},
    path::{Path, PathBuf},
    process::{Command, Stdio, exit},
    sync::mpsc::{self, Sender},
    thread,
    time::{Duration, Instant},
};

const VERSION: &'static str = "0.1.5";

const JSON_PATH: &'static str = "/data/data/com.termux/files/home/.local/share";
const JSON_NAME: &'static str = "pkgs.json";
const PKGS_CMD: &'static str = "/system/bin/pm list packages --user 0 -3 | cut -d: -f2";
const PATH_CMD: &'static str = "/system/bin/pm path --user 0";
const INTENT_CMD: &'static str = "/system/bin/pm resolve-activity --user 0";
const AAPT_CMD: &'static str = "aapt dump badging";
const EXTRA_PKGS: usize = 3;

#[derive(Debug, Serialize, Deserialize)]
struct Options {
    am: String,
    warn: bool,
    running_app: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            am: "old".to_string(),
            warn: true,
            running_app: None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq, PartialOrd, Hash, Clone)]
struct Package {
    pack_name: String,
    intent: Option<String>,
}

impl Default for Package {
    fn default() -> Self {
        Self {
            pack_name: "not available".to_string(),
            intent: None,
        }
    }
}

impl Package {
    fn settings() -> Self {
        let intent = Some("com.android.settings.Settings".to_string());
        let pack_name = "com.android.settings".to_string();
        Self { intent, pack_name }
    }

    fn playstore() -> Self {
        let intent = Some("com.android.vending.AssetBrowserActivity".to_string());
        let pack_name = "com.android.vending".to_string();
        Self { intent, pack_name }
    }

    fn youtube() -> Self {
        let intent =
            Some("com.google.android.youtube.app.honeycomb.Shell$HomeActivity".to_string());
        let pack_name = "com.google.android.youtube".to_string();
        Self { intent, pack_name }
    }
}

// NOTE: this assumes every command is successfull.which is very bad..
fn run_cmd(cmd: &str) -> anyhow::Result<String> {
    let shell = "/system/bin/sh";

    let out = Command::new(shell)
        .arg("-c")
        .arg(cmd)
        .stdout(Stdio::piped())
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()?
        .stdout;

    Ok(String::from_utf8(out)?)
}

fn run_cmd_err(cmd: &str) -> anyhow::Result<String> {
    let shell = "/system/bin/sh";

    let out = Command::new(shell)
        .arg("-c")
        .arg(cmd)
        .stdout(Stdio::piped())
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()?;

    if out.status.success() {
        Ok(String::from_utf8(out.stdout)?)
    } else {
        Err(anyhow::anyhow!(String::from_utf8(out.stderr)?))
    }
}

fn ret_pack_names(cmd: &str) -> anyhow::Result<Vec<String>> {
    let cmd_out = run_cmd(cmd)?;
    let pack_names = cmd_out.lines().map(|x| x.to_string()).collect();

    Ok(pack_names)
}

fn ret_path(pn: &str) -> anyhow::Result<String> {
    let cmd = format!("{PATH_CMD}  {pn} | cut -d: -f2");
    let cmd_out = run_cmd(&cmd)?;

    Ok(cmd_out)
}

fn ret_label(pn: &str) -> anyhow::Result<String> {
    let path = ret_path(pn)?;
    let cmd = format!("{AAPT_CMD} {path}");
    let cmd_out = run_cmd(&cmd)?;

    let label = cmd_out
        .lines()
        .find(|x| x.contains("application-label:"))
        .unwrap_or("No Label")
        .replace("application-label:", "")
        .replace("'", "");

    Ok(label)
}

fn ret_intent(pn: &str) -> anyhow::Result<Option<String>> {
    let cmd = format!("{INTENT_CMD} {pn}");
    let cmd_out = run_cmd(&cmd)?;

    match cmd_out.lines().find(|s| s.contains("name=")) {
        Some(s) => {
            let s = s.replace("name=", "").trim().to_string();
            Ok(Some(s))
        }
        None => Ok(None),
    }
}

fn launch_app(
    app: &str,
    map: &HashMap<String, Package>,
    am_opt_val: &str,
    is_repl: bool,
) -> anyhow::Result<()> {
    if let Some(package) = map.get(app) {
        let am_command = get_am_command(am_opt_val);
        let package_name = &package.pack_name;
        let intent = package.intent.as_deref().unwrap_or("none");

        let cmd = format!(
            "{} start --user 0 {}/'{}'", // '' to ignore expansions
            am_command, package_name, intent
        );

        eprintln!("  Launching `{}`", package.pack_name);
        let t = Instant::now();

        run_cmd_err(&cmd)?;

        let dur = t.elapsed().as_millis();
        println!("  Took {dur}ms");
        if !is_repl {
            exit(0);
        }
    } else {
        let sugg: Vec<_> = map
            .keys()
            .filter(|x| app.len() >= 2 && x.contains(app))
            .collect();

        if !sugg.is_empty() {
            eprintln!(" Did you mean {sugg:?}?");
            if !is_repl {
                exit(1);
            }
        } else if is_repl {
            eprintln!("  No app or command named '{app}' found");
            eprintln!("  Type 'ls' for listing all the apps");
            eprintln!("  Type 'help' for more information")
        } else {
            return Err(anyhow::anyhow!(
                "No app '{app}' found,use '-ls' option for listing apps"
            ));
        }
    }

    Ok(())
}

fn check_aapt() -> anyhow::Result<String> {
    match run_cmd_err("command -v aapt") {
        Err(_) => {
            eprintln!("  Installing aapt..");
            run_cmd_err("apt install aapt -y ")
        }
        Ok(_) => Ok(String::default()),
    }
}

fn initial_setup(path: &Path) -> anyhow::Result<HashMap<String, Package>> {
    check_aapt()?;

    spb::initial_bar_setup()?;

    println!("  Setting up packages for the first time...");
    println!();

    let mut hm: HashMap<String, Package> = HashMap::new();

    let pack_names = ret_pack_names(PKGS_CMD)?;
    let pack_len = pack_names.len();

    for (c, pack_name) in pack_names.into_iter().enumerate() {
        println!("  Adding `{pack_name}`");
        progress_bar(pack_len, c + 1);

        let mut label = ret_label(&pack_name)?.to_ascii_lowercase(); // most expensive
        let intent = ret_intent(&pack_name)?;

        if pack_name == "app.revanced.android.youtube" {
            // NOTE: Hardcoded only for youtube revanced
            label = "youtube_revanced".to_string();
        }

        let pkg = Package { pack_name, intent };

        hm.insert(label, pkg);
    }

    spb::restore_bar_setup()?;

    let playstore_label = "playstore".to_string();
    let settings_label = "settings".to_string();
    let youtube_label = "youtube".to_string();

    hm.insert(playstore_label, Package::playstore());
    hm.insert(settings_label, Package::settings());
    hm.insert(youtube_label, Package::youtube());

    fs::write(path, to_vec_pretty(&hm)?)?;
    Ok(hm)
}

fn retrive_packs(path: &Path) -> anyhow::Result<HashMap<String, Package>> {
    let s = fs::read_to_string(path)?;
    let m = from_str(&s)?;

    Ok(m)
}

fn handle_change(
    path: &Path,
    mut m: HashMap<String, Package>,
    new_pack_names: Vec<String>,
) -> anyhow::Result<HashMap<String, Package>> {
    let old_pack_names: HashSet<_> = m
        .values()
        .filter(|&p| {
            *p != Package::settings() && *p != Package::playstore() && *p != Package::youtube()
        })
        .map(|p| p.pack_name.clone())
        .collect();
    let new_pack_names: HashSet<_> = new_pack_names.into_iter().collect();

    // Find difference between old Package names and new Package names
    // to get the changed Package names.
    let added_pack_names: Vec<_> = new_pack_names.difference(&old_pack_names).collect();
    let removed_pack_names: Vec<_> = old_pack_names.difference(&new_pack_names).collect();

    for ap in added_pack_names {
        clear_line()?;
        eprint!("  added {}", ap.green());
        prompt(Some("\n"))?;

        let label = ret_label(ap)?.to_ascii_lowercase();
        let intent = ret_intent(ap)?;
        let pack = Package {
            pack_name: ap.to_string(),
            intent,
        };
        m.insert(label, pack);
    }

    for rp in removed_pack_names {
        clear_line()?;
        eprint!("  removed {}", rp.green());
        prompt(Some("\n"))?;

        if let Some((k, _)) = m.iter().find(|(_, v)| v.pack_name == *rp) {
            let key = k.clone();
            m.remove(&key);
        }
    }
    fs::write(path, to_vec_pretty(&m)?)?;
    Ok(m)
}

#[allow(unreachable_code)]
fn detect_change(tx: Sender<HashMap<String, Package>>, path: &Path) -> anyhow::Result<()> {
    loop {
        thread::sleep(Duration::from_secs(1));

        let s = fs::read_to_string(path)?;
        let m: HashMap<String, Package> = from_str(&s)?;

        let pack_names = ret_pack_names(PKGS_CMD)?;

        if m.len() - EXTRA_PKGS != pack_names.len() {
            let m = handle_change(path, m, pack_names)?;
            tx.send(m)?
        }
    }

    Ok(())
}

fn prompt(extra: Option<&str>) -> anyhow::Result<()> {
    if let Some(extra) = extra {
        print!("{extra}  {}", "> ".green_bold());
    } else {
        print!("  {}", "> ".green_bold());
    }

    stdout().flush()?;
    Ok(())
}

fn parse_args() -> anyhow::Result<Options> {
    let mut opts = Options::default();
    let am_opts = vec!["old", "system", "new"];

    let mut args = env::args();
    let program_name = args.next().unwrap_or("Non_existent_program".to_string());

    while let Some(opt) = args.next() {
        #[rustfmt::skip]
        match opt.as_str() {
            "--help" | "-h"     => messages::help_msg(&program_name),
            "--version" | "-v"  => messages::print_version(),
            "--no-warn" | "-nw" => opts.warn = false,
            "--list" | "-ls"    => {
                messages::print_apps()?;
                exit(0);
            }
            "--run" | "-r"      => {
                if let Some(app_name) = args.next() {
                    let app_name = app_name.to_ascii_lowercase().trim_start().to_string();
                    opts.running_app = Some(app_name);
                } else {
                    messages::no_val();
                }
            }
            "--am" | "-a"       => {
                if let Some(am_val) = args.next() {
                    opts.am = am_val;
                }
            }

            opt                 => messages::unknown_opt(opt),
        };
    }

    if !am_opts.contains(&opts.am.as_str()) {
        messages::unknown_val(&opts.am);
    }

    Ok(opts)
}
#[rustfmt::skip]
fn get_am_command(am_opt_val: &str) -> String {
    match am_opt_val {
        "old"    => "am".to_string(),
        "new"    => "termux-am".to_string(),
        "system" => "/system/bin/am".to_string(),
        _        => "am".to_string(),
    }
}

fn check_sdk_version() -> Result<()> {
    let sdk = run_cmd("/system/bin/getprop ro.build.version.sdk")?;

    if let Ok(sdk) = sdk.trim().parse::<u8>() {
        if sdk > 29 {
            messages::incompatible_warning(sdk);
        }
    };
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let opts = parse_args()?;
    let app_opt_val = opts.running_app;
    let am_opt_val = opts.am;
    let warn = opts.warn;

    let path = PathBuf::from(format!("{JSON_PATH}/{JSON_NAME}"));

    let (tx, rx) = mpsc::channel();

    let mut map: HashMap<String, Package> = if path.exists() {
        retrive_packs(&path)?
    } else {
        let t = Instant::now();

        DirBuilder::new().recursive(true).create(JSON_PATH)?;

        let m = initial_setup(&path)?;
        println!("  Took {}s", t.elapsed().as_secs());
        println!();
        m
    };

    thread::spawn(move || detect_change(tx, &path));

    #[rustfmt::skip]
    match am_opt_val.as_str() {
        "old" if warn    => messages::legacy_warning(),
        "system" if warn => check_sdk_version()?,
        _                => {}
    };

    if app_opt_val.is_none() {
        println!("  Enter app name to launch it");
        println!("  Type 'help' for more information");
        println!("  Exit by typing 'q");
        println!();
    }

    loop {
        match rx.try_recv() {
            Ok(m) => {
                map = m;
                continue;
            }
            _ => {}
        }
        // Command line app launch
        if let Some(app) = &app_opt_val {
            launch_app(app, &map, &am_opt_val, false)?;
        }

        // for repl
        let mut repl_input = String::new();
        prompt(None)?;
        io::stdin().read_line(&mut repl_input)?;
        let repl_input = repl_input.trim().to_ascii_lowercase();

        #[rustfmt::skip]
        match repl_input.as_str() {
            "quit" | "q"           => break,
            "list" | "ls"          => messages::print_apps()?,
            "help" | "h"           => messages::repl_help(),
            "clear" | "cl"         => clear_screen()?,
            app if !app.is_empty() => launch_app(app, &map, &am_opt_val, true)?,
            _                      => {}
        };
    }
    Ok(())
}

mod messages {
    use super::*;

    pub fn legacy_warning() {
        eprintln!("  {}: Using the legacy am.", "Warning".yellow_bold());
        eprintln!("  launching apps will be slow");
        println!();
    }

    pub fn repl_help() {
        let link = "https://github.com/BayonetArch/tx_launch";

        println!("  Launch apps by typing out their labels.");
        println!();

        println!("  Commands:");
        println!("    ls,list     list all apps");
        println!("    cl,clear    clear the repl");
        println!("    q,quit      exit the repl");
        println!("    h,help      print this help message");
        println!();

        println!(
            "  {}: Launching apps will be slow if you are using default am.",
            "NOTE".blue_bold()
        );
        println!("  consider changing the am");
        println!("  more information can be found here:\n  {}", link.green(),);
        println!();
    }

    pub fn help_msg(name: &str) {
        println!("tx_launch v{VERSION}");
        println!("a cli tool to launch android apps\n");

        println!("usage");
        println!("  {} [flags] ...", name);
        println!();

        println!("options");
        println!("  -a, --am  <value>            specify which am to use (default old)");
        println!("  -r, --run <app_name>         run an app directly");
        println!("  -ls, --list                  list available apps");
        println!("  -nw, --no-warn               supress all the warning messages");
        println!("  -h, --help                   print this help message");
        println!("  -v, --version                print the binary version");

        println!();
        println!("available am values");
        println!("  old    use the legacy termux am slow but included in stable termux releases");
        println!(
            "  system use the system am (/system/bin/am) fastest but incompatible with android 11+"
        );
        println!(
            "  new    use the new termux am (github action builds only) which is faster than old termux"
        );
        println!();
        println!("example");
        println!("  '{} --am new --run tiktok'", name);
        println!("the above example uses new am and runs tiktok");
        exit(0);
    }

    pub fn print_version() {
        println!("v{VERSION}");
        exit(0);
    }

    pub fn unknown_opt(s: &str) {
        eprintln!(
            "  Unknown option '{}'  try '--help' for more  information",
            s
        );
        exit(1);
    }

    pub fn unknown_val(s: &str) {
        eprintln!(
            "  Unknown value '{}'  try '--help' for more  information",
            s
        );
        exit(1);
    }

    pub fn no_val() {
        eprintln!("  No value provided try '--help' for more  information");
        exit(1);
    }

    pub fn incompatible_warning(sdk: u8) {
        eprintln!(
            "  {}: SDK VERSION is higher than expected for system am",
            "Warning".yellow_bold()
        );
        eprintln!("  Expected: 29 or lower");
        eprintln!("  Found: {}", sdk);
        eprintln!("  Note: apps may not run  on this version.");
        eprintln!();
    }

    pub fn print_apps() -> anyhow::Result<()> {
        let path = PathBuf::from(format!("{JSON_PATH}/{JSON_NAME}"));
        let map: HashMap<String, Package>;

        if !path.exists() {
            map = initial_setup(&path)?;
        } else {
            let apps = fs::read_to_string(&path)?;
            map = from_str(&apps)?;
        }

        println!();
        println!("  {}", "—".repeat(20));
        map.keys().for_each(|x| println!("  {x}"));
        println!("  {}", "—".repeat(20));
        println!();

        Ok(())
    }
}
