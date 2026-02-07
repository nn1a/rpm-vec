use crate::mcp::protocol::Tool;
use serde_json::json;

/// Get all available MCP tools
pub fn get_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "search_packages".to_string(),
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
            name: "get_package_info".to_string(),
            description: "Get detailed information about a specific package".to_string(),
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
            name: "list_repositories".to_string(),
            description: "List all indexed repositories with package counts".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        Tool {
            name: "compare_versions".to_string(),
            description: "Compare two RPM package versions using the rpmvercmp algorithm"
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "version1": {
                        "type": "string",
                        "description": "First version (format: epoch:version-release or version-release)"
                    },
                    "version2": {
                        "type": "string",
                        "description": "Second version (format: epoch:version-release or version-release)"
                    }
                },
                "required": ["version1", "version2"]
            }),
        },
        Tool {
            name: "get_repository_stats".to_string(),
            description: "Get statistics for a specific repository".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "repo": {
                        "type": "string",
                        "description": "Repository name"
                    }
                },
                "required": ["repo"]
            }),
        },
    ]
}
