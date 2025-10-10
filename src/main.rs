use serde::{Deserialize, Serialize};
use serde_json::{from_str, to_vec_pretty};
use std::{
    collections::{HashMap, HashSet},
    env::{self},
    fs::{self, DirBuilder},
    io::{self, Write, stdout},
    path::{Path, PathBuf},
    process::{Command, Stdio, exit},
    sync::{
        LazyLock, Mutex,
        mpsc::{self, Sender},
    },
    thread,
    time::{Duration, Instant},
};

const VERSION: &'static str = "0.1.0";

const JSON_PATH: &'static str = "/data/data/com.termux/files/home/.local/share";
const JSON_NAME: &'static str = "pkgs.json";
const PKGS_CMD: &'static str = "/system/bin/pm list packages --user 0 -3 | cut -d: -f2";
const PATH_CMD: &'static str = "/system/bin/pm path --user 0";
const INTENT_CMD: &'static str = "/system/bin/pm resolve-activity --user 0";
const AAPT_CMD: &'static str = "aapt dump badging";

static ACTMANAGER: LazyLock<Mutex<Am>> = std::sync::LazyLock::new(|| Mutex::new(Am::default()));

#[derive(Debug, PartialEq)]
struct Am(String);

impl Am {
    fn system() -> Self {
        Self("/system/bin/am".to_string())
    }
    fn termux_old() -> Self {
        Self("am".to_string())
    }
    fn termux_new() -> Self {
        Self("termux-am".to_string())
    }
}

impl Default for Am {
    fn default() -> Self {
        Self::termux_old()
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
}

struct EscAttr {}

impl EscAttr {
    fn red<'a>(bold: bool) -> &'a str {
        if !bold { "\x1b[0;31m" } else { "\x1b[1;31m" }
    }

    fn green<'a>(bold: bool) -> &'a str {
        if !bold { "\x1b[0;32m" } else { "\x1b[1;32m" }
    }

    fn yellow<'a>(bold: bool) -> &'a str {
        if !bold { "\x1b[0;33m" } else { "\x1b[1;33m" }
    }

    fn _underline<'a>() -> &'a str {
        "\x1b[4m"
    }

    fn clear_line<'a>() -> &'a str {
        "\x1b[033[2k\r"
    }
    fn clear_screen<'a>() -> &'a str {
        "\x1b[2J\x1b[H"
    }

    fn none<'a>() -> &'a str {
        "\x1b[0m"
    }
}

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

fn launch_app(match_str: &str, hm: &HashMap<String, Package>, am: &Am) -> anyhow::Result<bool> {
    let mut b = true;

    if match_str == "list" || match_str == "ls" {
        print_apps(hm);
        return Ok(b);
    } else if match_str == "help" {
        repl_help();
        return Ok(b);
    } else if match_str == "clear" {
        println!("{}", EscAttr::clear_screen());
        return Ok(b);
    }

    let default = Package::default();
    let p = hm.get(&match_str.to_string()).unwrap_or(&default);

    if *p != default {
        let cmd = format!(
            "{} start --user 0 {}/{}",
            am.0,
            p.pack_name,
            p.intent.as_deref().unwrap_or("none")
        );

        eprintln!("  launching `{}`", p.pack_name);
        let t1 = Instant::now();
        run_cmd(&cmd)?;
        let dur = t1.elapsed().as_millis();
        println!("  took {dur} millisecs");
    } else {
        let sugg: Vec<_> = hm
            .keys()
            .filter(|x| match_str.len() >= 2 && x.contains(match_str))
            .collect();

        if !sugg.is_empty() {
            eprintln!("  Did you mean ? {:?}", sugg);
        } else {
            eprintln!("  no app '{}' found", match_str);
            eprintln!("  type 'list|ls' to list all the apps");
            b = false;
        }
    }
    Ok(b)
}

fn initial_setup(path: &Path) -> anyhow::Result<HashMap<String, Package>> {
    println!("  setting up packages for the first time...");
    println!("  it will take some time so please wait");

    let mut hm: HashMap<String, Package> = HashMap::new();

    let pack_names = ret_pack_names(PKGS_CMD)?;
    for pack_name in pack_names {
        let label = ret_label(&pack_name)?.to_ascii_lowercase(); // most expensive
        let intent = ret_intent(&pack_name)?;
        let pkg = Package { pack_name, intent };

        hm.insert(label, pkg);
    }

    let playstore_label = "playstore".to_string();
    let settings_label = "settings".to_string();
    hm.insert(playstore_label, Package::playstore());
    hm.insert(settings_label, Package::settings());

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
    new_packs: Vec<String>,
) -> anyhow::Result<HashMap<String, Package>> {
    let old_packs: HashSet<_> = m
        .values()
        .filter(|&p| *p != Package::settings() && *p != Package::playstore())
        .map(|p| p.pack_name.clone())
        .collect();
    let new_packs: HashSet<_> = new_packs.into_iter().collect();

    let added: Vec<_> = new_packs.difference(&old_packs).collect();
    let removed: Vec<_> = old_packs.difference(&new_packs).collect();

    for ap in added {
        eprint!(
            "{}{}  added{} '{ap}'",
            EscAttr::clear_line(),
            EscAttr::green(false),
            EscAttr::none()
        );
        prompt("\n")?;

        let label = ret_label(ap)?.to_ascii_lowercase();
        let intent = ret_intent(ap)?;
        let pack = Package {
            pack_name: ap.to_string(),
            intent,
        };
        m.insert(label, pack);
    }

    for rp in removed {
        eprint!(
            "{}{}  removed{} '{rp}'",
            EscAttr::clear_line(),
            EscAttr::red(true),
            EscAttr::none()
        );
        prompt("\n")?;

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

        if m.len() - 2 != pack_names.len() {
            let m = handle_change(path, m, pack_names)?;
            tx.send(m)?
        }
    }

    Ok(())
}

fn prompt(extra: &str) -> anyhow::Result<()> {
    print!("{}  {}>{} ", extra, EscAttr::green(true), EscAttr::none());
    stdout().flush()?;
    Ok(())
}

fn help_msg(name: &str) {
    println!("tx_launch v{VERSION}");
    println!("a cli tool to launch android apps\n");

    println!("usage");
    println!("  {} [flags] ...", name);
    println!();

    println!("options");
    println!("  -a, --am        specify which am to use (default old)");
    println!("  -r, --run       run an app directly");
    println!("  -h, --help      print this help message");
    println!("  -v, --version   print the binary version");

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
fn print_version() {
    println!("v{VERSION}");
    exit(0);
}

fn repl_help() {
    let link = "https://github.com/BayonetArch/tx_launch";

    println!();
    println!("  launch apps by typing out their labels.");
    println!("  type 'list|ls' to print all the app labels.");
    println!("  type 'clear' to clear the repl");
    println!("  if launching apps is too slow consider changing the am");
    println!(
        "  guide for 'am':{}{}{}",
        EscAttr::green(false),
        link,
        EscAttr::none()
    );
    println!();
}

fn incompatible_warning(sdk: u8) {
    eprintln!(
        "  {}Warning{}: SDK VERSION is higher than expected for system am",
        EscAttr::yellow(true),
        EscAttr::none()
    );
    eprintln!("  expected: 29,got:{}", sdk);
    eprintln!("  Keep in mind that apps might not run");
}

fn legacy_warning() {
    eprintln!(
        "  {}Warning{}: Using the legacy am.",
        EscAttr::yellow(true),
        EscAttr::none(),
    );
    eprintln!("  launching apps will be slow");
    println!();
}

fn unknown_opt(s: &str) {
    eprintln!(
        "  Unknown option '{}'  try '--help' for more  information",
        s
    );
    exit(1);
}

fn unkown_val(s: &str) {
    eprintln!(
        "  Unknown value '{}'  try '--help' for more  information",
        s
    );
    exit(1);
}

fn no_val() {
    eprintln!("  No value provided try '--help' for more  information");
    exit(1);
}

fn handle_am(args: &Vec<String>, i: usize) -> String {
    let mut app = String::new();

    if let Some(s) = args.get(i) {
        match s.as_str() {
            "new" => {
                let mut a = ACTMANAGER.lock().unwrap();
                *a = Am::termux_new();
            }
            "old" => {
                let mut a = ACTMANAGER.lock().unwrap();
                *a = Am::termux_old();
            }

            "system" => {
                let mut a = ACTMANAGER.lock().unwrap();
                *a = Am::system();
            }

            x => unkown_val(x),
        }
        if let Some(sec_flag) = args.get(i + 1) {
            match sec_flag.as_str() {
                "--run" | "-r" => app = handle_run(args, i + 2),
                x => unknown_opt(x),
            }
        }
    } else {
        no_val();
    }
    app
}

fn handle_run(args: &Vec<String>, i: usize) -> String {
    let mut app = String::new();

    if let Some(a) = args.get(i) {
        match a.as_str() {
            x => app = x.to_ascii_lowercase(),
        }

        if let Some(sec_flag) = args.get(i + 1) {
            match sec_flag.as_str() {
                "--am" | "-a" => {
                    let _ = handle_am(&args, i + 2);
                }
                x => unknown_opt(x),
            }
        }
    } else {
        no_val();
    }
    app
}
//TODO: somehow get rid of this bruteforced arg parsing
fn parse_args() -> anyhow::Result<String> {
    let mut app = String::new();

    let args: Vec<_> = env::args().collect();

    if args.len() < 2 {
        return Ok(app);
    }

    if let Some(x) = args.get(1) {
        match x.as_str() {
            "--am" | "-a" => app = handle_am(&args, 2),
            "--run" | "-r" => app = handle_run(&args, 2),

            "--help" | "-h" => help_msg(&args[0]),
            "--version" | "-v" => print_version(),

            x => unknown_opt(x),
        }
    }
    Ok(app)
}
fn print_apps(map: &HashMap<String, Package>) {
    let labels: Vec<_> = map.keys().collect();
    println!("  {:#?}", labels);
}

fn main() -> anyhow::Result<()> {
    let app = parse_args()?;
    let am = ACTMANAGER.lock().unwrap();

    let path_str = format!("{JSON_PATH}/{JSON_NAME}");
    let path = PathBuf::from(path_str);

    let (tx, rx) = mpsc::channel();

    let mut map: HashMap<String, Package> = if path.exists() {
        retrive_packs(&path)?
    } else {
        let t = Instant::now();

        let mut dir = DirBuilder::new();
        dir.recursive(true);
        dir.create(JSON_PATH)?;

        let m = initial_setup(&path)?;
        println!("  took {} secs", t.elapsed().as_secs());
        m
    };

    thread::spawn(move || detect_change(tx, &path));

    if *am == Am::default() {
        legacy_warning();
    }

    if *am == Am::system() {
        let sdk = run_cmd(
            "/system/bin/getprop getprop ro.build.version.sdk
",
        )?;

        if let Ok(sdk) = sdk.parse::<u8>() {
            if sdk > 29 {
                incompatible_warning(sdk);
            }
        }
    }

    if app.is_empty() {
        println!("  enter app name to launch it");
        println!("  type 'help' for more information");
        println!("  you can exit by entering 'q'");
    }

    loop {
        match rx.try_recv() {
            Ok(m) => {
                map = m;
                continue;
            }
            _ => {}
        }

        if !app.is_empty() {
            if !launch_app(&app, &map, &am)? {
                exit(1);
            };
            exit(0);
        }

        let mut match_str = String::new();
        prompt("")?;
        io::stdin().read_line(&mut match_str)?;
        let match_str = match_str.trim().to_ascii_lowercase();

        if match_str == "q" {
            break;
        }
        if !match_str.is_empty() {
            launch_app(&match_str, &map, &am)?;
        }
    }
    Ok(())
}
