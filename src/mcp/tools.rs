use crate::mcp::protocol::Tool;
use serde_json::json;

/// Get all available MCP tools
pub fn get_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "rpm_search".to_string(),
            description: "Natural language semantic search for RPM packages using vector embeddings. Best for exploratory queries like 'SSL encryption library' or 'image processing tool'. For exact name/field matching, use rpm_find instead."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Natural language search query in English (e.g., 'compression library', 'network packet capture tool')"
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
            description: "Get detailed information about a specific RPM package including version, requires, provides, and file list".to_string(),
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
                    },
                    "include_files": {
                        "type": "boolean",
                        "description": "Include file list in output (default: true, set false to omit)",
                        "default": true
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
        Tool {
            name: "rpm_file_search".to_string(),
            description: "Search for RPM packages that contain a specific file. Returns the package name, version, and the matched file path. Use this to answer 'which package provides /usr/bin/python3?' type questions.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path or filename to search (e.g., '/usr/bin/python3', 'libssl.so.3')"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results (default 20)",
                        "default": 20
                    }
                },
                "required": ["path"]
            }),
        },
        Tool {
            name: "rpm_find".to_string(),
            description: "Find RPM packages using structured filters with wildcard support (* and ?). Multiple filters are ANDed together.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Package name pattern (e.g., 'lib*ssl*', 'python?')"
                    },
                    "summary": {
                        "type": "string",
                        "description": "Summary keyword pattern"
                    },
                    "provides": {
                        "type": "string",
                        "description": "Provides capability pattern (e.g., 'libssl.so*')"
                    },
                    "requires": {
                        "type": "string",
                        "description": "Requires dependency pattern (e.g., 'libcrypto*')"
                    },
                    "file": {
                        "type": "string",
                        "description": "File path pattern (e.g., '/usr/bin/python*')"
                    },
                    "arch": {
                        "type": "string",
                        "description": "Architecture filter"
                    },
                    "repo": {
                        "type": "string",
                        "description": "Repository filter"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum results (default 50)",
                        "default": 50
                    }
                }
            }),
        },
    ]
}
