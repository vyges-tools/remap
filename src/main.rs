//! vyges-remap — a Loom engine for **file-level multi-output technology
//! re-mapping**. It wraps the `vyges-emap` driver (mockturtle `emap`): given an
//! AIGER netlist + a technology genlib, it runs a single-output baseline and a
//! multi-output pass and reports the before/after cell/area delta, writing the
//! remapped Verilog netlist. `--describe` advertises the structured contract to
//! `vyges mcp`; `--json` emits the result envelope.
//!
//! The `vyges-emap` driver is resolved via `$VYGES_EMAP` (default `vyges-emap`
//! on PATH); `--liberty` is converted to a genlib via ABC (`$VYGES_ABC`).
//!
//! House style: std + serde_json only.

use serde_json::{json, Value};
use std::process::Command;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const USAGE: &str = "\
vyges-remap — file-level multi-output technology re-mapping (mockturtle emap)

usage:
  vyges-remap emap --aig <in.aig> (--genlib <g> | --liberty <lib>) [--top T] [-o out.v] [--json]
  vyges-remap --describe        structured tool contract (for `vyges mcp`)
  vyges-remap --version | --help

Runs the vyges-emap driver twice (single-output baseline + multi-output) over the
AIGER netlist and reports the before/after cell/area delta. Resolve the driver via
$VYGES_EMAP (default: `vyges-emap` on PATH).
";

/// Cell/area/multi-output counts parsed from a `vyges-emap` stats sidecar.
#[derive(Clone, Copy)]
struct Stats {
    gates: f64,
    area: f64,
    mo: f64,
}

fn parse_stats(text: &str) -> Option<Stats> {
    let v: Value = serde_json::from_str(text).ok()?;
    Some(Stats {
        gates: v.get("gates")?.as_f64()?,
        area: v.get("area")?.as_f64()?,
        mo: v.get("multioutput_gates").and_then(Value::as_f64).unwrap_or(0.0),
    })
}

fn pct(before: f64, after: f64) -> Option<f64> {
    if before == 0.0 {
        None
    } else {
        Some((after - before) / before * 100.0)
    }
}

fn stat_json(s: &Stats) -> Value {
    json!({ "cell_count": s.gates, "cell_area": s.area, "multioutput_gates": s.mo })
}

fn delta_json(b: f64, a: f64) -> Value {
    json!({ "before": b, "after": a, "abs": a - b, "pct": pct(b, a) })
}

/// The `--describe` payload the mcp registry parses into a typed MCP tool.
fn describe() -> Value {
    json!({
        "name": "remap",
        "summary": "File-level multi-output technology re-mapping (mockturtle emap): AIGER + genlib -> mapped netlist + before/after cell/area delta.",
        "invocation": {
            "args_template": ["emap", "--aig", "{aig}", "--genlib", "{genlib}"],
            "optional": [
                { "arg": "top", "flag": "--top" },
                { "arg": "out", "flag": "-o" }
            ],
            "emits_json": true
        },
        "inputs": {
            "type": "object",
            "required": ["aig", "genlib"],
            "properties": {
                "aig":    { "type": "string", "description": "AIGER netlist (.aig) of the logic to remap" },
                "genlib": { "type": "string", "description": "technology genlib (multi-output cells are derived from xor/maj gates)" },
                "top":    { "type": "string", "description": "top module name (metadata)" },
                "out":    { "type": "string", "description": "path to write the remapped Verilog netlist" }
            }
        },
        "artifacts": [ { "role": "netlist", "field": "out_netlist" } ]
    })
}

fn tail(s: &str) -> String {
    let mut l: Vec<&str> = s.lines().rev().take(6).collect();
    l.reverse();
    l.join("\n")
}

/// Run `vyges-emap` once; parse its stats sidecar. Never panics.
fn run_emap(emap: &str, aig: &str, genlib: &str, out: &str, stats_path: &str, multioutput: bool) -> Result<Stats, String> {
    let mut cmd = Command::new(emap);
    cmd.args(["--aig", aig, "--genlib", genlib, "-o", out, "--stats", stats_path]);
    if multioutput {
        cmd.arg("--multioutput");
    }
    let output = cmd
        .output()
        .map_err(|e| format!("cannot run '{emap}': {e} (set $VYGES_EMAP or install vyges-emap)"))?;
    if !output.status.success() {
        return Err(format!(
            "vyges-emap exited {}: {}",
            output.status.code().unwrap_or(-1),
            tail(&String::from_utf8_lossy(&output.stderr))
        ));
    }
    let text = std::fs::read_to_string(stats_path).map_err(|e| format!("no stats from vyges-emap: {e}"))?;
    parse_stats(&text).ok_or_else(|| "vyges-emap stats unparsable".to_string())
}

/// Liberty → genlib via ABC. Multi-output cells are derived by the tech library
/// from single-output gates (xor/maj), so the Liberty must carry those.
fn liberty_to_genlib(liberty: &str, out_genlib: &str) -> Result<(), String> {
    let abc = std::env::var("VYGES_ABC").unwrap_or_else(|_| "abc".into());
    let script = format!("read_liberty {liberty}; write_genlib {out_genlib}");
    let output = Command::new(&abc)
        .args(["-q", &script])
        .output()
        .map_err(|e| format!("cannot run abc: {e} (set $VYGES_ABC or install abc)"))?;
    if !output.status.success() || !std::path::Path::new(out_genlib).exists() {
        return Err(format!("abc Liberty->genlib failed: {}", tail(&String::from_utf8_lossy(&output.stderr))));
    }
    Ok(())
}

fn fail(want_json: bool, top: &str, msg: &str) -> i32 {
    if want_json {
        println!("{:#}", json!({ "schema": "vyges-remap/1.0", "status": "error", "top": top, "error": msg }));
    } else {
        eprintln!("vyges-remap: {msg}");
    }
    1
}

fn emit(want_json: bool, r: &Value) {
    if want_json {
        println!("{r:#}");
        return;
    }
    let pctf = |v: &Value| v.as_f64().map(|x| format!("{x:+.2}%")).unwrap_or_else(|| "-".into());
    let cc = &r["delta"]["cell_count"];
    let ca = &r["delta"]["cell_area"];
    println!("[remap] {}   {} multi-output cells", r["top"].as_str().unwrap_or("?"), r["multioutput_cells"]);
    println!("  cells: {} -> {}  ({})", cc["before"], cc["after"], pctf(&cc["pct"]));
    println!("  area:  {} -> {}  ({})", ca["before"], ca["after"], pctf(&ca["pct"]));
    println!("  netlist: {}", r["out_netlist"].as_str().unwrap_or("-"));
}

fn cmd_emap(args: &[String]) -> i32 {
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str);
    let want_json = args.iter().any(|a| a == "--json");
    let Some(aig) = opt("--aig") else {
        eprintln!("vyges-remap emap: --aig <in.aig> is required");
        return 2;
    };
    let top = opt("--top").unwrap_or("top");
    let out = opt("-o").or_else(|| opt("--out")).unwrap_or("remap_out.v");

    let work = std::env::temp_dir().join(format!("vyges-remap-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&work);
    let ws = |n: &str| work.join(n).to_string_lossy().to_string();

    // genlib: direct (--genlib) or derived from Liberty via ABC (--liberty).
    let genlib: String = if let Some(g) = opt("--genlib") {
        g.to_string()
    } else if let Some(lib) = opt("--liberty") {
        let g = ws("tech.genlib");
        if let Err(e) = liberty_to_genlib(lib, &g) {
            let _ = std::fs::remove_dir_all(&work);
            return fail(want_json, top, &format!("Liberty->genlib: {e}"));
        }
        g
    } else {
        eprintln!("vyges-remap emap: --genlib <g> or --liberty <lib> is required");
        return 2;
    };

    let emap = std::env::var("VYGES_EMAP").unwrap_or_else(|_| "vyges-emap".into());
    let base = match run_emap(&emap, aig, &genlib, &ws("base.v"), &ws("base.json"), false) {
        Ok(s) => s,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&work);
            return fail(want_json, top, &e);
        }
    };
    let mo = match run_emap(&emap, aig, &genlib, out, &ws("mo.json"), true) {
        Ok(s) => s,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&work);
            return fail(want_json, top, &e);
        }
    };
    let _ = std::fs::remove_dir_all(&work);

    let result = json!({
        "schema": "vyges-remap/1.0",
        "status": "ok",
        "top": top,
        "aig": aig,
        "genlib": genlib,
        "out_netlist": out,
        "before": stat_json(&base),
        "after": stat_json(&mo),
        "delta": {
            "cell_count": delta_json(base.gates, mo.gates),
            "cell_area":  delta_json(base.area, mo.area)
        },
        "multioutput_cells": mo.mo
    });
    emit(want_json, &result);
    0
}

fn run(args: Vec<String>) -> i32 {
    match args.first().map(String::as_str) {
        Some("--describe") => {
            println!("{:#}", describe());
            0
        }
        Some("-V") | Some("--version") => {
            println!("vyges-remap {VERSION}");
            0
        }
        None | Some("-h") | Some("--help") => {
            print!("{USAGE}");
            0
        }
        Some("emap") => cmd_emap(&args[1..]),
        Some(other) => {
            eprintln!("vyges-remap: unknown command '{other}' (try --help)");
            2
        }
    }
}

fn main() {
    std::process::exit(run(std::env::args().skip(1).collect()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_stats_reads_emap_sidecar() {
        let s = parse_stats(r#"{"gates":256,"area":1024.0,"multioutput_gates":1}"#).unwrap();
        assert_eq!(s.gates, 256.0);
        assert_eq!(s.area, 1024.0);
        assert_eq!(s.mo, 1.0);
        assert!(parse_stats("not json").is_none());
        // multioutput_gates optional → defaults 0
        assert_eq!(parse_stats(r#"{"gates":1,"area":2}"#).unwrap().mo, 0.0);
    }

    #[test]
    fn delta_computes_pct() {
        let d = delta_json(257.0, 256.0);
        assert_eq!(d["abs"], json!(-1.0));
        assert!((d["pct"].as_f64().unwrap() - (-0.389)).abs() < 0.01);
        assert_eq!(delta_json(0.0, 5.0)["pct"], Value::Null); // div-by-zero → null
    }

    #[test]
    fn describe_is_a_valid_engine_contract() {
        let d = describe();
        assert_eq!(d["name"], "remap");
        assert_eq!(d["invocation"]["emits_json"], true);
        assert_eq!(d["invocation"]["args_template"][0], "emap");
        // required inputs match the template's {tokens}
        let req = d["inputs"]["required"].as_array().unwrap();
        assert!(req.contains(&json!("aig")) && req.contains(&json!("genlib")));
        assert_eq!(d["artifacts"][0]["field"], "out_netlist");
    }
}
