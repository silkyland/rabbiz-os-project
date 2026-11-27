//! Headless `--cli` and `--mcp` interface for `rabbiz-search`.

use de_config::prefs::{search_engine_label, search_url, SEARCH_ENGINES};
use de_config::Prefs;
use rabbiz_app_kit::mcp::Tool;
use serde_json::{json, Value};

use crate::apps::{discover_apps, filter_apps};
use crate::web::fetch_web_hits;

pub static TOOLS: &[Tool] = &[
    Tool {
        name: "search_apps",
        description: "Search installed desktop applications by name or id.",
        input_schema: r#"{"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}"#,
        handler: mcp_search_apps,
    },
    Tool {
        name: "search_web",
        description: "Search the web and return structured title/domain/url/snippet results.",
        input_schema: r#"{"type":"object","properties":{"query":{"type":"string"},"engine":{"type":"string"}},"required":["query"]}"#,
        handler: mcp_search_web,
    },
    Tool {
        name: "search_engines",
        description: "List supported search engines and the active default engine.",
        input_schema: r#"{"type":"object","properties":{}}"#,
        handler: mcp_search_engines,
    },
    Tool {
        name: "set_search_engine",
        description: "Set the default web search engine (google, perplexity, duckduckgo, yahoo, live).",
        input_schema: r#"{"type":"object","properties":{"engine":{"type":"string"}},"required":["engine"]}"#,
        handler: mcp_set_search_engine,
    },
];

fn mcp_search_apps(args: &Value) -> Result<String, String> {
    let q = args
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let all = discover_apps();
    let hits: Vec<Value> = filter_apps(&all, q)
        .into_iter()
        .map(|a| json!({ "id": a.id, "name": a.name, "exec": a.exec }))
        .collect();
    Ok(json!({ "query": q, "apps": hits }).to_string())
}

fn mcp_search_web(args: &Value) -> Result<String, String> {
    let q = args
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    let prefs = Prefs::load();
    let engine = args
        .get("engine")
        .and_then(Value::as_str)
        .unwrap_or(&prefs.search_engine);
    let hits = fetch_web_hits(q).unwrap_or_default();
    let items: Vec<Value> = hits
        .into_iter()
        .map(|h| {
            json!({
                "title": h.title,
                "url": h.url,
                "domain": h.domain,
                "snippet": h.snippet,
            })
        })
        .collect();
    Ok(json!({
        "query": q,
        "engine": engine,
        "engine_label": search_engine_label(engine),
        "browser_url": search_url(engine, q),
        "results": items,
    })
    .to_string())
}

fn mcp_search_engines(_args: &Value) -> Result<String, String> {
    let prefs = Prefs::load();
    let engines: Vec<Value> = SEARCH_ENGINES
        .iter()
        .map(|(id, name, prefix)| {
            json!({
                "id": id,
                "name": name,
                "url_prefix": prefix,
                "active": prefs.search_engine.eq_ignore_ascii_case(id),
            })
        })
        .collect();
    Ok(json!({ "default": prefs.search_engine, "engines": engines }).to_string())
}

fn mcp_set_search_engine(args: &Value) -> Result<String, String> {
    let id = args
        .get("engine")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_lowercase();
    if !SEARCH_ENGINES.iter().any(|(k, _, _)| *k == id) {
        return Err(format!("unknown search engine '{id}'"));
    }
    let mut prefs = Prefs::load();
    prefs.search_engine.clone_from(&id);
    prefs.save().map_err(|e| format!("save failed: {e}"))?;
    Ok(json!({ "ok": true, "engine": id, "label": search_engine_label(&id) }).to_string())
}

pub fn run(args: &[String]) -> i32 {
    let Some(cmd) = args.first().map(String::as_str) else {
        eprintln!("usage: rabbiz-search --cli <apps <q>|web <q>|engines|engine [set <id>]>");
        return 2;
    };
    match cmd {
        "apps" => {
            let q = args[1..].join(" ");
            let all = discover_apps();
            let hits: Vec<Value> = filter_apps(&all, &q)
                .into_iter()
                .map(|a| json!({ "id": a.id, "name": a.name, "exec": a.exec }))
                .collect();
            println!("{}", json!({ "query": q, "apps": hits }));
            0
        }
        "web" => {
            let q = args[1..].join(" ");
            let prefs = Prefs::load();
            match fetch_web_hits(&q) {
                Ok(hits) => {
                    let items: Vec<Value> = hits
                        .into_iter()
                        .map(|h| {
                            json!({
                                "title": h.title,
                                "url": h.url,
                                "domain": h.domain,
                                "snippet": h.snippet,
                            })
                        })
                        .collect();
                    println!(
                        "{}",
                        json!({
                            "query": q,
                            "engine": prefs.search_engine,
                            "browser_url": search_url(&prefs.search_engine, &q),
                            "results": items,
                        })
                    );
                    0
                }
                Err(err) => {
                    eprintln!("rabbiz-search: {err}");
                    1
                }
            }
        }
        "engines" => {
            let prefs = Prefs::load();
            let engines: Vec<Value> = SEARCH_ENGINES
                .iter()
                .map(|(id, name, _)| {
                    json!({
                        "id": id,
                        "name": name,
                        "active": prefs.search_engine.eq_ignore_ascii_case(id),
                    })
                })
                .collect();
            println!("{}", json!({ "default": prefs.search_engine, "engines": engines }));
            0
        }
        "engine" => {
            let mut prefs = Prefs::load();
            if args.get(1).map(String::as_str) == Some("set") {
                let id = args.get(2).map(String::as_str).unwrap_or_default().to_lowercase();
                if !SEARCH_ENGINES.iter().any(|(k, _, _)| *k == id) {
                    eprintln!("unknown search engine '{id}'");
                    return 2;
                }
                prefs.search_engine = id;
                if let Err(e) = prefs.save() {
                    eprintln!("save failed: {e}");
                    return 1;
                }
            }
            println!(
                "{}",
                json!({
                    "engine": prefs.search_engine,
                    "label": search_engine_label(&prefs.search_engine),
                })
            );
            0
        }
        other => {
            eprintln!("unknown subcommand '{other}'");
            2
        }
    }
}
