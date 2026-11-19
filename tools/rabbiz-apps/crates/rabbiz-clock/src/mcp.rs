//! MCP stdio server for `rabbiz-clock --mcp`, sharing the same alarm store as
//! the GUI and CLI.

use crate::cli::{alarm_add_text, alarm_remove_text, alarm_set_enabled, alarms_text, now_text};

fn mcp_str<'a>(args: &'a serde_json::Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("missing or invalid '{key}'"))
}

fn tool_alarms_list(_args: &serde_json::Value) -> Result<String, String> {
    alarms_text()
}

fn tool_alarm_add(args: &serde_json::Value) -> Result<String, String> {
    alarm_add_text(mcp_str(args, "time")?)
}

fn tool_alarm_remove(args: &serde_json::Value) -> Result<String, String> {
    alarm_remove_text(mcp_str(args, "id")?)
}

fn tool_alarm_enable(args: &serde_json::Value) -> Result<String, String> {
    let id = mcp_str(args, "id")?;
    let enabled = args
        .get("enabled")
        .and_then(|v| v.as_bool())
        .ok_or("missing or invalid 'enabled' (want boolean)")?;
    alarm_set_enabled(id, enabled)
}

fn tool_now(_args: &serde_json::Value) -> Result<String, String> {
    Ok(now_text())
}

static TOOLS: &[rabbiz_app_kit::mcp::Tool] = &[
    rabbiz_app_kit::mcp::Tool {
        name: "alarms_list",
        description: "List all alarms (id, HH:MM, on/off, firedDay).",
        input_schema: r#"{"type":"object","properties":{}}"#,
        handler: tool_alarms_list,
    },
    rabbiz_app_kit::mcp::Tool {
        name: "alarm_add",
        description: "Add a daily alarm at HH:MM (24h), enabled. Same store as the Clock GUI.",
        input_schema: r#"{"type":"object","properties":{"time":{"type":"string","description":"HH:MM, e.g. \"07:30\""}},"required":["time"]}"#,
        handler: tool_alarm_add,
    },
    rabbiz_app_kit::mcp::Tool {
        name: "alarm_remove",
        description: "Delete an alarm by id.",
        input_schema: r#"{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}"#,
        handler: tool_alarm_remove,
    },
    rabbiz_app_kit::mcp::Tool {
        name: "alarm_enable",
        description: "Enable or disable an alarm by id.",
        input_schema: r#"{"type":"object","properties":{"id":{"type":"string"},"enabled":{"type":"boolean"}},"required":["id","enabled"]}"#,
        handler: tool_alarm_enable,
    },
    rabbiz_app_kit::mcp::Tool {
        name: "now",
        description: "Current local time and the next enabled alarm.",
        input_schema: r#"{"type":"object","properties":{}}"#,
        handler: tool_now,
    },
];

pub fn serve() -> ! {
    rabbiz_app_kit::mcp::serve(TOOLS, "rabbiz-clock", "0.1.0")
}
