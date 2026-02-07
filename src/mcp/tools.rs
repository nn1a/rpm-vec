use crate::mcp::protocol::Tool;
use serde_json::json;

/// Get all available MCP tools
pub fn get_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "rpm_search".to_string(),
            description: "Search RPM packages by name, description, or semantic similarity"
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query (package name, description, or natural language)"
                    },
                    "arch": {
                        "type": "string",
                        "description": "Filter by architecture (e.g., x86_64, aarch64, noarch)"
                    },
                    "repo": {
                        "type": "string",
                        "description": "Filter by repository name"
                    },
                    "top_k": {
                        "type": "integer",
                        "description": "Maximum number of results to return",
                        "default": 10
                    }
                },
                "required": ["query"]
            }),
        },
        Tool {
            name: "rpm_package_info".to_string(),
            description: "Get detailed information about a specific RPM package".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Package name"
                    },
                    "arch": {
                        "type": "string",
                        "description": "Architecture (optional, helps narrow down results)"
                    },
                    "repo": {
                        "type": "string",
                        "description": "Repository name (optional)"
                    }
                },
                "required": ["name"]
            }),
        },
        Tool {
            name: "rpm_repositories".to_string(),
            description: "List all indexed RPM repositories with package counts".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
    ]
}
