//! MCP stdio server for `rabbiz-calculator --mcp`, backed by the same
//! expression model as the GUI and the CLI.

use rabbiz_app_kit::mcp::Tool;

use crate::cli::eval_to_text;

static TOOLS: &[Tool] = &[Tool {
    name: "eval",
    description: "Evaluate a calculator expression and return the formatted \
                  result (e.g. \"2+3*4\" → \"14\", \"50%+2\", \"1e3\"). \
                  Returns an error message for invalid input or division by zero.",
    input_schema: r#"{"type":"object","properties":{"expr":{"type":"string"}},"required":["expr"]}"#,
    handler: eval_tool,
}];

fn eval_tool(args: &serde_json::Value) -> Result<String, String> {
    let expr = args
        .get("expr")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing 'expr' string argument".to_string())?;
    eval_to_text(expr)
}

/// Serve MCP on stdin/stdout until EOF. Never returns.
pub fn serve() -> ! {
    rabbiz_app_kit::mcp::serve(TOOLS, "rabbiz-calculator", "0.1.0")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_table_shape() {
        assert_eq!(TOOLS.len(), 1);
        assert_eq!(TOOLS[0].name, "eval");
        assert!(serde_json::from_str::<serde_json::Value>(TOOLS[0].input_schema).is_ok());
    }

    #[test]
    fn eval_tool_handler() {
        let ok = eval_tool(&serde_json::json!({"expr": "2+3*4"})).unwrap();
        assert_eq!(ok, "14");
        let err = eval_tool(&serde_json::json!({"expr": "5/0"})).unwrap_err();
        assert!(err.contains("divide"));
        assert!(eval_tool(&serde_json::json!({})).is_err());
        assert!(eval_tool(&serde_json::json!({"expr": 42})).is_err());
    }
}
