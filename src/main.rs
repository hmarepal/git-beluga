//! `git-beluga`: raise a pixel-art beluga from a GitHub account's activity.

mod activity;
mod collect;
mod output;
mod whale;

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use activity::{Activity, Language, Mood, Snapshot};
use whale::{H, Pattern, Rgb, W, Whale};

const USAGE: &str = "\
git-beluga: raise a pixel-art beluga from a GitHub account's activity

USAGE:
  git-beluga draw <login> [OPTIONS]      collect through `gh`, then draw
  git-beluga collect <login> [-o FILE]   save a snapshot as JSON (stdout by default)
  git-beluga render <snapshot.json> [OPTIONS]
  git-beluga sheet [-o FILE]             every pattern, mood and size, as one PNG

OPTIONS:
  -o, --out DIR       folder for beluga.gif and beluga.png [default: beluga]
      --pattern NAME  rings|aurora|patches|tide|swirl|ombre [default: rings]
      --mood NAME     force rest|drift|cruise|greet|swim|leap instead of today's
      --require-private  draw nothing if the token can't see any private repositories";

struct Options {
    out: Option<PathBuf>,
    pattern: Pattern,
    mood: Option<Mood>,
    require_private: bool,
}

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        out: None,
        pattern: Pattern::Rings,
        mood: None,
        require_private: false,
    };
    let mut args = args.iter();
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or(format!("{flag} needs a value"));
        match flag.as_str() {
            "-o" | "--out" => options.out = Some(PathBuf::from(value()?)),
            "--pattern" => {
                let name = value()?;
                options.pattern = Pattern::parse(name).ok_or(format!("unknown pattern {name}"))?;
            }
            "--mood" => {
                let name = value()?;
                options.mood = Some(Mood::parse(name).ok_or(format!("unknown mood {name}"))?);
            }
            "--require-private" => options.require_private = true,
            other => return Err(format!("unknown option {other}\n\n{USAGE}")),
        }
    }
    Ok(options)
}

fn draw_snapshot(snapshot: &Snapshot, options: &Options) -> Result<(), String> {
    if options.require_private && snapshot.private_repos_counted == 0 {
        return Err(format!(
            "this token can't see any private repositories, so nothing was drawn. \
             Give it access to All repositories with Metadata: Read-only ({} public repos were visible).",
            snapshot.repos_counted
        ));
    }
    let activity = Activity::from_snapshot(snapshot);
    let mood = options.mood.unwrap_or_else(|| activity.mood());
    let beluga = Whale::new(&activity, &snapshot.languages, options.pattern);
    let (frames, delay) = whale::animate(&beluga, mood);
    let dir = options
        .out
        .clone()
        .unwrap_or_else(|| PathBuf::from("beluga"));
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let gif = output::encode_gif(&frames, W, H, delay)?;
    let png = output::encode_png(&frames[0], W, H, None)?;
    fs::write(dir.join("beluga.gif"), &gif).map_err(|e| e.to_string())?;
    fs::write(dir.join("beluga.png"), &png).map_err(|e| e.to_string())?;
    let languages: Vec<String> = snapshot
        .languages
        .iter()
        .map(|l| format!("{} {:.1}%", l.name, l.share * 100.0))
        .collect();
    println!(
        "{}: {} on {}, {} contributions, best streak {} weeks",
        snapshot.login,
        mood.name(),
        snapshot.observed_on,
        activity.total,
        activity.longest_streak_weeks,
    );
    println!(
        "{} repos averaged ({} private): {}",
        snapshot.repos_counted,
        snapshot.private_repos_counted,
        languages.join(", "),
    );
    println!("wrote {}", dir.join("beluga.gif").display());
    Ok(())
}

/// A grid of every pattern, every mood, and three sizes, for the README.
fn contact_sheet() -> whale::Frame {
    const COLUMNS: usize = 3;
    let language = |name: &str, color: &str, share: f32| Language {
        name: name.into(),
        color: color.into(),
        share,
    };
    let languages = [
        language("Rust", "#dea584", 0.3),
        language("TypeScript", "#3178c6", 0.25),
        language("C++", "#f34b7d", 0.2),
        language("Go", "#00ADD8", 0.15),
        language("Shell", "#89e051", 0.1),
    ];
    let adult = Activity {
        total: 3000,
        longest_streak_weeks: 30,
        recent_streak_weeks: 8,
        ..Activity::quiet()
    };
    let mut cells: Vec<whale::Frame> = Pattern::ALL
        .iter()
        .map(|&(_, pattern)| {
            whale::still(&Whale::new(&adult, &languages, pattern), Mood::Swim, 0.2)
        })
        .collect();
    for mood in Mood::ALL {
        let t = if mood == Mood::Leap { 0.13 } else { 0.25 };
        cells.push(whale::still(
            &Whale::new(&adult, &languages, Pattern::Rings),
            mood,
            t,
        ));
    }
    for total in [5, 200, 20_000] {
        let grown = Activity {
            total,
            ..adult.clone()
        };
        cells.push(whale::still(
            &Whale::new(&grown, &languages, Pattern::Rings),
            Mood::Swim,
            0.2,
        ));
    }
    let rows = cells.len().div_ceil(COLUMNS);
    let (sheet_w, sheet_h) = (W * COLUMNS, H * rows);
    let mut sheet = vec![None; sheet_w * sheet_h];
    for (i, cell) in cells.iter().enumerate() {
        let (ox, oy) = ((i % COLUMNS) * W, (i / COLUMNS) * H);
        for y in 0..H {
            for x in 0..W {
                sheet[(oy + y) * sheet_w + ox + x] = cell[y * W + x];
            }
        }
    }
    sheet
}

fn run(args: &[String]) -> Result<(), String> {
    let command = args.first().map(String::as_str).unwrap_or("");
    let target = args.get(1).map(String::as_str);
    match (command, target) {
        ("draw", Some(login)) => {
            draw_snapshot(&collect::collect(login)?, &parse_options(&args[2..])?)
        }
        ("render", Some(path)) => {
            let text = fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
            let snapshot = serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
            draw_snapshot(&snapshot, &parse_options(&args[2..])?)
        }
        ("collect", Some(login)) => {
            let options = parse_options(&args[2..])?;
            let json = serde_json::to_string_pretty(&collect::collect(login)?)
                .map_err(|e| e.to_string())?;
            match options.out {
                Some(path) => {
                    fs::write(&path, json).map_err(|e| format!("{}: {e}", path.display()))
                }
                None => {
                    println!("{json}");
                    Ok(())
                }
            }
        }
        ("sheet", _) => {
            let options = parse_options(&args[1..])?;
            let path = options.out.unwrap_or_else(|| PathBuf::from("sheet.png"));
            let sheet = contact_sheet();
            let png = output::encode_png(
                &sheet,
                W * 3,
                sheet.len() / (W * 3),
                Some(Rgb(0x0d, 0x11, 0x17)),
            )?;
            fs::write(&path, png).map_err(|e| format!("{}: {e}", path.display()))
        }
        _ => Err(USAGE.to_owned()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
